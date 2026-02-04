//! AI Agent decision logic
//!
//! This module provides:
//! - Intent analysis from user prompts
//! - Tool selection (DSP vs Neural vs Both)
//! - Confidence scoring
//! - Conversation context management

mod decision;
mod intent;

pub use decision::{Agent, AgentResponse, ToolDecision, ToolType};
pub use intent::{Intent, IntentAnalyzer};
