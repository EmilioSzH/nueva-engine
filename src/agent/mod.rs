//! AI Agent decision logic
//!
//! This module provides:
//! - Intent analysis from user prompts
//! - Tool selection (DSP vs Neural vs Both)
//! - Confidence scoring
//! - Conversation context management
//! - Reference resolution
//! - Undo/redo management
//! - Explanation generation

mod context;
mod decision;
mod explain;
mod intent;
mod reference;
mod safety;
mod undo;

pub use context::{
    ActionType, AgentAction, ConversationContext, EffectFocus, EffectRef, Message, MessageRole,
    ModifyOrAdd, ParameterChange, UserPreferences,
};
pub use decision::{Agent, AgentResponse, ToolDecision, ToolType};
pub use explain::{explain_full_chain, explain_last_action};
pub use intent::{Intent, IntentAnalyzer};
pub use reference::resolve_reference;
pub use safety::{
    AudioAnalysis, RecommendationPriority, SafetyCheckResult, SafetyChecker, SafetyIssue,
    SafetyMitigation, SafetyRecommendation,
};
pub use undo::{UndoManager, UndoableAction};
