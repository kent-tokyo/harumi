#[cfg(feature = "anthropic")]
pub mod anthropic;
pub mod echo;
#[cfg(feature = "openai")]
pub mod openai;

#[cfg(feature = "anthropic")]
pub use anthropic::AnthropicTranslator;
pub use echo::EchoTranslator;
#[cfg(feature = "openai")]
pub use openai::OpenAiTranslator;

/// Default overall timeout for one provider HTTP request.
#[cfg(any(feature = "anthropic", feature = "openai"))]
pub(crate) const DEFAULT_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Build the HTTP client shared by the LLM providers.
///
/// Without a timeout a stalled API connection would hang the translation forever.
#[cfg(any(feature = "anthropic", feature = "openai"))]
pub(crate) fn http_client(timeout: std::time::Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(timeout)
        .build()
        // Building only fails if the TLS backend cannot initialise; fall back to
        // reqwest's defaults rather than panicking.
        .unwrap_or_default()
}
