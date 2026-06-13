use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::{Error, Result, Translator};

const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";
const DEFAULT_MODEL: &str = "gpt-4o-mini";

/// OpenAI-compatible translation provider.
///
/// Works with OpenAI, Azure OpenAI, Groq, DeepSeek, Ollama, or any service
/// implementing the `/v1/chat/completions` schema.
///
/// # Example
/// ```no_run
/// use harumi_ai::providers::OpenAiTranslator;
/// let _t = OpenAiTranslator::builder()
///     .api_key(std::env::var("OPENAI_API_KEY").unwrap())
///     .build();
/// ```
pub struct OpenAiTranslator {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: String,
    system_prompt_template: String,
}

/// Builder for [`OpenAiTranslator`].
#[derive(Default)]
pub struct OpenAiTranslatorBuilder {
    endpoint: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    system_prompt_template: Option<String>,
}

impl OpenAiTranslatorBuilder {
    /// Override the API endpoint (default: OpenAI).
    pub fn endpoint(mut self, e: impl Into<String>) -> Self {
        self.endpoint = Some(e.into());
        self
    }

    pub fn api_key(mut self, k: impl Into<String>) -> Self {
        self.api_key = Some(k.into());
        self
    }

    /// Model name (default: `"gpt-4o-mini"`).
    pub fn model(mut self, m: impl Into<String>) -> Self {
        self.model = Some(m.into());
        self
    }

    /// Override the system prompt template.
    ///
    /// Available placeholders:
    /// - `{target_lang}` — BCP-47 target language tag
    /// - `{source_lang}` — replaced with e.g. `"from Japanese "` or `""` (auto-detect)
    pub fn system_prompt_template(mut self, t: impl Into<String>) -> Self {
        self.system_prompt_template = Some(t.into());
        self
    }

    pub fn build(self) -> OpenAiTranslator {
        OpenAiTranslator {
            client: reqwest::Client::new(),
            endpoint: self.endpoint.unwrap_or_else(|| DEFAULT_ENDPOINT.to_owned()),
            api_key: self.api_key.unwrap_or_default(),
            model: self.model.unwrap_or_else(|| DEFAULT_MODEL.to_owned()),
            system_prompt_template: self
                .system_prompt_template
                .unwrap_or_else(default_system_prompt),
        }
    }
}

fn default_system_prompt() -> String {
    "You are a professional document translator. \
     The user provides a JSON object with a \"pages\" array. \
     Each page has a \"blocks\" array where every block has \"id\", \"type\" \
     (\"h1\"–\"h6\" or \"paragraph\"), and \"text\". \
     Translate each block's \"text\" {source_lang}to {target_lang}. \
     Return ONLY a valid JSON object mirroring the input structure: \
     {\"pages\": [{\"blocks\": [{\"id\": <number>, \"text\": \"<translated>\"}, ...]}, ...]}. \
     Preserve every id, maintain the same page order, and return exactly the same \
     number of pages and blocks as the input. \
     Do not add commentary, markdown fences, or any text outside the JSON."
        .to_owned()
}

impl OpenAiTranslator {
    pub fn builder() -> OpenAiTranslatorBuilder {
        OpenAiTranslatorBuilder::default()
    }

    fn build_system(&self, target_lang: &str, source_lang: Option<&str>) -> String {
        let src_part = match source_lang {
            Some(s) if !s.is_empty() && s != "auto" => format!("from {s} "),
            _ => String::new(),
        };
        self.system_prompt_template
            .replace("{source_lang}", &src_part)
            .replace("{target_lang}", target_lang)
    }
}

// ── serde types ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: String,
}

// ── Translator impl ─────────────────────────────────────────────────────────

#[async_trait]
impl Translator for OpenAiTranslator {
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
            let req = ChatRequest {
                model: self.model.clone(),
                messages: vec![
                    ChatMessage { role: "system".into(), content: system.clone() },
                    ChatMessage { role: "user".into(), content: text.clone() },
                ],
                temperature: 0.3,
            };

            let resp: ChatResponse = self
                .client
                .post(&self.endpoint)
                .bearer_auth(&self.api_key)
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
                .choices
                .into_iter()
                .next()
                .ok_or_else(|| Error::Translator("LLM returned empty choices".into()))?
                .message
                .content;

            results.push(raw);
        }

        Ok(results)
    }
}
