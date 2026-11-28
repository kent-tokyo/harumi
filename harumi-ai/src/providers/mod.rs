#[cfg(feature = "anthropic")]
pub mod anthropic;
pub mod echo;
#[cfg(feature = "openai")]
pub mod openai;
pub mod segment;

#[cfg(feature = "anthropic")]
pub use anthropic::AnthropicTranslator;
pub use echo::EchoTranslator;
#[cfg(feature = "openai")]
pub use openai::OpenAiTranslator;
pub use segment::{SegmentAdapter, SegmentTranslator};
