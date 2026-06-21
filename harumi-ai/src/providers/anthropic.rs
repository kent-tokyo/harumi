use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};

use crate::{
    Error, Result, Translator,
    layout_repair::{LayoutCorrection, VisionProvider, VisionRepairRequest},
    prompts::translation_system_prompt,
};

const DEFAULT_ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_MODEL: &str = "claude-sonnet-4-6";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Anthropic Claude translation provider.
///
/// # Example
/// ```no_run
/// use harumi_ai::providers::AnthropicTranslator;
/// let _t = AnthropicTranslator::builder()
///     .api_key(std::env::var("ANTHROPIC_API_KEY").unwrap())
///     .build();
/// ```
pub struct AnthropicTranslator {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: String,
    max_tokens: u32,
    system_prompt_template: String,
}

/// Builder for [`AnthropicTranslator`].
#[derive(Default)]
pub struct AnthropicTranslatorBuilder {
    endpoint: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    max_tokens: Option<u32>,
    system_prompt_template: Option<String>,
}

impl AnthropicTranslatorBuilder {
    /// Override the API endpoint (default: `https://api.anthropic.com/v1/messages`).
    pub fn endpoint(mut self, e: impl Into<String>) -> Self {
        self.endpoint = Some(e.into());
        self
    }

    pub fn api_key(mut self, k: impl Into<String>) -> Self {
        self.api_key = Some(k.into());
        self
    }

    /// Model ID (default: `"claude-sonnet-4-6"`).
    pub fn model(mut self, m: impl Into<String>) -> Self {
        self.model = Some(m.into());
        self
    }

    /// Maximum output tokens (default: `4096`).
    pub fn max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = Some(n);
        self
    }

    /// Override the system prompt template.
    ///
    /// Available placeholders:
    /// - `{target_lang}` — BCP-47 target language tag
    /// - `{source_lang}` — replaced with e.g. `"from Japanese"` or `"(auto-detect source language)"`
    pub fn system_prompt_template(mut self, t: impl Into<String>) -> Self {
        self.system_prompt_template = Some(t.into());
        self
    }

    pub fn build(self) -> AnthropicTranslator {
        AnthropicTranslator {
            client: reqwest::Client::new(),
            endpoint: self.endpoint.unwrap_or_else(|| DEFAULT_ENDPOINT.to_owned()),
            api_key: self.api_key.unwrap_or_default(),
            model: self.model.unwrap_or_else(|| DEFAULT_MODEL.to_owned()),
            max_tokens: self.max_tokens.unwrap_or(16_000),
            system_prompt_template: self.system_prompt_template.unwrap_or_default(),
        }
    }
}

impl AnthropicTranslator {
    pub fn builder() -> AnthropicTranslatorBuilder {
        AnthropicTranslatorBuilder::default()
    }

    fn build_system(&self, target_lang: &str, source_lang: Option<&str>) -> String {
        if self.system_prompt_template.is_empty() {
            translation_system_prompt(target_lang, source_lang)
        } else if self.system_prompt_template.contains("{target_lang}")
            || self.system_prompt_template.contains("{source_lang}")
        {
            let src_part = match source_lang {
                Some(s) if !s.is_empty() && s != "auto" => format!("from {s} "),
                _ => String::new(),
            };
            self.system_prompt_template
                .replace("{source_lang}", &src_part)
                .replace("{target_lang}", target_lang)
        } else {
            self.system_prompt_template.clone()
        }
    }
}

// ── serde types ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: String,
    messages: Vec<Message<'a>>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: String,
}

#[derive(Serialize)]
struct VisionMessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<VisionMessage<'a>>,
}

#[derive(Serialize)]
struct VisionMessage<'a> {
    role: &'a str,
    content: Vec<VisionContent<'a>>,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum VisionContent<'a> {
    #[serde(rename = "text")]
    Text { text: &'a str },
    #[serde(rename = "image")]
    Image { source: VisionImageSource },
}

#[derive(Serialize)]
struct VisionImageSource {
    #[serde(rename = "type")]
    source_type: &'static str,
    media_type: &'static str,
    data: String,
}

#[derive(Deserialize)]
struct VisionCorrectionResponse {
    corrections: Vec<LayoutCorrection>,
}

// ── Translator impl ─────────────────────────────────────────────────────────

#[async_trait]
impl Translator for AnthropicTranslator {
    async fn translate(
        &self,
        texts: &[String],
        target_lang: &str,
        source_lang: Option<&str>,
    ) -> Result<Vec<String>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let system = self.build_system(target_lang, source_lang);
        let mut results = Vec::with_capacity(texts.len());

        for text in texts {
            let req = MessagesRequest {
                model: &self.model,
                max_tokens: self.max_tokens,
                system: system.clone(),
                messages: vec![Message {
                    role: "user",
                    content: text,
                }],
            };

            let resp: MessagesResponse = self
                .client
                .post(&self.endpoint)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .json(&req)
                .send()
                .await
                .map_err(|e| Error::Translator(e.to_string()))?
                .error_for_status()
                .map_err(|e| Error::Translator(e.to_string()))?
                .json()
                .await
                .map_err(|e| Error::Translator(e.to_string()))?;

            let raw = resp
                .content
                .into_iter()
                .next()
                .ok_or_else(|| Error::Translator("Claude returned empty content".into()))?
                .text;

            results.push(raw);
        }

        Ok(results)
    }
}

#[async_trait]
impl VisionProvider for AnthropicTranslator {
    async fn repair_layout(
        &self,
        request: VisionRepairRequest<'_>,
    ) -> Result<Vec<LayoutCorrection>> {
        let source_png = general_purpose::STANDARD.encode(request.source_png);
        let translated_png = general_purpose::STANDARD.encode(request.translated_png);
        let text = format!(
            "Compare the source PDF page and translated PDF page.\n\
             Return corrections only for translated text that visibly overflows, collides, \
             covers images, or breaks a table/value field.\n\
             Keep the language as {}. Preserve numbers, units, and codes exactly.\n\
             Source language hint: {}.\n\
             Geometry issues:\n{}\n\n\
             Return ONLY JSON: {{\"corrections\":[{{\"page\":{},\"id\":<number>,\"text\":\"<corrected>\",\"reason\":\"<short>\"}}]}}",
            request.target_lang,
            request.source_lang.unwrap_or("auto"),
            request.geometry_issues_json,
            request.page,
        );
        let system = "You are a PDF translation layout repair engine. Use the images to verify geometry diagnostics. Do not rewrite unaffected text.";
        let req = VisionMessagesRequest {
            model: &self.model,
            max_tokens: self.max_tokens,
            system,
            messages: vec![VisionMessage {
                role: "user",
                content: vec![
                    VisionContent::Text {
                        text: "Source page image:",
                    },
                    VisionContent::Image {
                        source: VisionImageSource {
                            source_type: "base64",
                            media_type: "image/png",
                            data: source_png,
                        },
                    },
                    VisionContent::Text {
                        text: "Translated page image:",
                    },
                    VisionContent::Image {
                        source: VisionImageSource {
                            source_type: "base64",
                            media_type: "image/png",
                            data: translated_png,
                        },
                    },
                    VisionContent::Text { text: &text },
                ],
            }],
        };

        let resp: MessagesResponse = self
            .client
            .post(&self.endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&req)
            .send()
            .await
            .map_err(|e| Error::Translator(e.to_string()))?
            .error_for_status()
            .map_err(|e| Error::Translator(e.to_string()))?
            .json()
            .await
            .map_err(|e| Error::Translator(e.to_string()))?;

        let raw = resp
            .content
            .into_iter()
            .next()
            .ok_or_else(|| Error::Translator("Claude returned empty vision content".into()))?
            .text;
        let json_str = {
            let s = raw.trim();
            let s = s.strip_prefix("```json").unwrap_or(s);
            let s = s.strip_prefix("```").unwrap_or(s);
            let s = s.strip_suffix("```").unwrap_or(s);
            s.trim()
        };
        let parsed: VisionCorrectionResponse = serde_json::from_str(json_str).map_err(|e| {
            Error::Translator(format!("Claude vision JSON invalid: {e}. Raw: {json_str}"))
        })?;
        Ok(parsed.corrections)
    }
}
