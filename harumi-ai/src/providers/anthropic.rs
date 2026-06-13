use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{prompts::translation_system_prompt, Error, Result, Translator};

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
            max_tokens: self.max_tokens.unwrap_or(4096),
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
                messages: vec![Message { role: "user", content: text }],
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
