// output.rs — TranslateOutput and supporting diagnostic types

use harumi::PageFitSummary;

use crate::{TranslationMode, quality::QualityResult};

/// Options controlling which debug artifacts are produced alongside the translated PDF.
///
/// All fields default to `false`.  Set the relevant flag before calling [`translate_pdf`].
#[derive(Debug, Clone, Default)]
pub struct DebugOptions {
    /// Emit a JSON string summarising per-page layout quality.
    pub layout_report: bool,
    /// Emit a JSON string listing all detected collisions.
    pub collision_report: bool,
    /// Emit a copy of the original PDF with colored debug overlay rectangles drawn using
    /// harumi's [`harumi::PageHandle::add_fit_debug_overlay`].
    pub overlay_pdf: bool,
    /// Record the AI correction history (which lines were corrected in each round).
    pub correction_history: bool,
}

/// The full result of a [`translate_pdf`](crate::translate_pdf) call.
///
/// Previously the function returned `Result<Vec<u8>>`; from v0.2.0 it returns
/// `Result<TranslateOutput>` so callers can inspect quality diagnostics and
/// optionally extract debug artifacts without a separate call.
///
/// The translated PDF bytes are in [`TranslateOutput::pdf_bytes`].
pub struct TranslateOutput {
    /// The translated PDF bytes. This is the primary output.
    pub pdf_bytes: Vec<u8>,
    /// Per-page quality reports and an overall gate verdict.
    pub quality: TranslateQuality,
    /// Debug artifacts, populated according to [`DebugOptions`].
    /// `None` when all `DebugOptions` flags are `false`.
    pub debug: Option<DebugArtifacts>,
}

/// Quality diagnostics attached to a [`TranslateOutput`].
pub struct TranslateQuality {
    /// Per-page layout quality reports.
    pub pages: Vec<PageQualityReport>,
    /// Aggregate quality gate verdict across all pages.
    pub overall: QualityResult,
    /// Number of AI correction rounds actually executed.
    pub correction_rounds: usize,
    /// Translation mode that was actually used (relevant when `Auto` was requested).
    pub mode_used: TranslationMode,
}

/// Layout quality summary for a single translated page.
pub struct PageQualityReport {
    /// 1-based page number.
    pub page_num: u32,
    /// Aggregate fitting/collision summary for this page.
    pub summary: PageFitSummary,
}

/// Debug artifacts optionally produced alongside the translated PDF.
pub struct DebugArtifacts {
    /// JSON-serialised `Vec<PageQualityReport>` (if [`DebugOptions::layout_report`] is `true`).
    pub layout_report_json: Option<String>,
    /// JSON-serialised collision list (if [`DebugOptions::collision_report`] is `true`).
    pub collision_report_json: Option<String>,
    /// Debug overlay PDF with colored boxes for source, placement, and collision
    /// rectangles (if [`DebugOptions::overlay_pdf`] is `true`).
    pub debug_overlay_pdf: Option<Vec<u8>>,
    /// Per-round correction history (if [`DebugOptions::correction_history`] is `true`).
    pub correction_history: Vec<CorrectionRound>,
}

/// Summary of one AI correction round in the layout repair loop.
#[derive(Debug, Clone)]
pub struct CorrectionRound {
    /// 1-based round number.
    pub round: usize,
    /// Number of text lines the AI was asked to shorten in this round.
    pub lines_sent_to_ai: usize,
    /// Number of corrections the AI returned.
    pub corrections_applied: usize,
    /// Number of pages that had at least one problem before this round.
    pub pages_with_problems: usize,
}
