use std::{path::PathBuf, process::Stdio, time::Duration};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Controls post-translation layout repair.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LayoutRepairMode {
    /// Do not run AI layout repair.
    Off,
    /// Repair using geometry diagnostics only.
    GeometryOnly,
    /// Repair with geometry first, then use vision only for pages that still fail.
    #[default]
    GeometryThenVision,
    /// Rasterize and compare every page with a vision provider.
    VisionAllPages,
}

/// Options for Poppler-based page rasterization.
#[derive(Debug, Clone)]
pub struct RasterizeOptions {
    /// Poppler command name or absolute path.
    pub command: String,
    /// Output image resolution.
    pub dpi: u32,
    /// Per-page timeout.
    pub timeout_per_page: Duration,
}

impl Default for RasterizeOptions {
    fn default() -> Self {
        Self {
            command: "pdftoppm".to_owned(),
            dpi: 144,
            timeout_per_page: Duration::from_secs(30),
        }
    }
}

/// One replacement text returned by a geometry or vision repair pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutCorrection {
    /// 1-based page number.
    pub page: u32,
    /// Line or region id on that page.
    pub id: usize,
    /// Corrected translated text.
    pub text: String,
    /// Provider-supplied reason, if available.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Input sent to a vision repair provider for one page.
pub struct VisionRepairRequest<'a> {
    /// 1-based page number.
    pub page: u32,
    /// PNG rendering of the source page.
    pub source_png: &'a [u8],
    /// PNG rendering of the translated page.
    pub translated_png: &'a [u8],
    /// Geometry issue JSON for this page.
    pub geometry_issues_json: &'a str,
    /// Target language tag.
    pub target_lang: &'a str,
    /// Optional source language tag.
    pub source_lang: Option<&'a str>,
}

/// Provider-agnostic vision repair interface.
#[async_trait]
pub trait VisionProvider: Send + Sync {
    /// Return corrected translations for layout issues visible on the page.
    async fn repair_layout(
        &self,
        request: VisionRepairRequest<'_>,
    ) -> Result<Vec<LayoutCorrection>>;
}

/// Rasterize one PDF page to PNG using Poppler's `pdftoppm`.
pub(crate) async fn rasterize_page_png(
    pdf_bytes: &[u8],
    page: u32,
    options: &RasterizeOptions,
) -> Result<Vec<u8>> {
    let nonce = format!(
        "{}_{}_{}",
        std::process::id(),
        page,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let base = std::env::temp_dir().join(format!("harumi_ai_{nonce}"));
    let input = base.with_extension("pdf");
    let prefix = PathBuf::from(format!("{}_page", base.display()));

    std::fs::write(&input, pdf_bytes)?;

    let mut child = tokio::process::Command::new(&options.command)
        .arg("-f")
        .arg(page.to_string())
        .arg("-l")
        .arg(page.to_string())
        .arg("-r")
        .arg(options.dpi.to_string())
        .arg("-png")
        .arg(&input)
        .arg(&prefix)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::Translator(format!("failed to start {}: {e}", options.command)))?;

    let wait = tokio::time::timeout(options.timeout_per_page, child.wait()).await;
    let status = match wait {
        Ok(r) => r.map_err(|e| Error::Translator(format!("{} failed: {e}", options.command)))?,
        Err(_) => {
            let _ = child.kill().await;
            return Err(Error::Translator(format!(
                "{} timed out rasterizing page {}",
                options.command, page
            )));
        }
    };

    let _ = std::fs::remove_file(&input);

    if !status.success() {
        return Err(Error::Translator(format!(
            "{} failed rasterizing page {}",
            options.command, page
        )));
    }

    let candidates = [
        PathBuf::from(format!("{}-{}.png", prefix.display(), page)),
        PathBuf::from(format!("{}-1.png", prefix.display())),
    ];
    for candidate in candidates {
        if let Ok(bytes) = std::fs::read(&candidate) {
            let _ = std::fs::remove_file(&candidate);
            return Ok(bytes);
        }
    }

    Err(Error::Translator(format!(
        "{} did not produce a PNG for page {}",
        options.command, page
    )))
}
