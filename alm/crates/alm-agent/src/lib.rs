//! ALM Agent — LLM integration for ALM code generation and self-healing.
//!
//! Core capabilities:
//! - Generate structured prompts from alm.yaml + intent
//! - Self-heal loop: compile → error → retry with context
//! - Module summary generation (.alm.meta)
//! - Agent lifecycle management

pub mod prompt;
pub mod selfheal;
pub mod meta;

pub use prompt::PromptBuilder;
pub use selfheal::SelfHealLoop;
pub use meta::ModuleSummary;
