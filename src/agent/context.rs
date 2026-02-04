//! Conversation context management
//!
//! Tracks conversation state, messages, and recent actions.
//! Implements §7.1 from the spec.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use super::decision::ToolType;

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message ID
    pub id: String,

    /// Who sent the message
    pub role: MessageRole,

    /// Message content
    pub content: String,

    /// When the message was sent
    pub timestamp: DateTime<Utc>,

    /// Associated action (if agent message resulted in action)
    pub action_id: Option<String>,
}

impl Message {
    pub fn user(content: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role: MessageRole::User,
            content: content.to_string(),
            timestamp: Utc::now(),
            action_id: None,
        }
    }

    pub fn agent(content: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role: MessageRole::Agent,
            content: content.to_string(),
            timestamp: Utc::now(),
            action_id: None,
        }
    }

    pub fn with_action(mut self, action_id: &str) -> Self {
        self.action_id = Some(action_id.to_string());
        self
    }
}

/// Who sent the message
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Agent,
    System,
}

/// An action taken by the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAction {
    /// Unique action ID
    pub id: String,

    /// Type of action
    pub action_type: ActionType,

    /// Tool used (DSP, Neural, Both)
    pub tool: ToolType,

    /// Human-readable description
    pub description: String,

    /// Effect that was affected (if any)
    pub affected_effect: Option<EffectRef>,

    /// Model used (for neural actions)
    pub model_name: Option<String>,

    /// Model parameters (for neural actions)
    pub model_params: Option<HashMap<String, serde_json::Value>>,

    /// Parameter changes made (for DSP actions)
    pub parameter_changes: Vec<ParameterChange>,

    /// Reasoning for the decision
    pub reasoning: String,

    /// When the action was taken
    pub timestamp: DateTime<Utc>,
}

impl AgentAction {
    pub fn new(action_type: ActionType, tool: ToolType, description: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            action_type,
            tool,
            description: description.to_string(),
            affected_effect: None,
            model_name: None,
            model_params: None,
            parameter_changes: Vec::new(),
            reasoning: String::new(),
            timestamp: Utc::now(),
        }
    }

    pub fn with_effect(mut self, effect: EffectRef) -> Self {
        self.affected_effect = Some(effect);
        self
    }

    pub fn with_model(mut self, name: &str, params: HashMap<String, serde_json::Value>) -> Self {
        self.model_name = Some(name.to_string());
        self.model_params = Some(params);
        self
    }

    pub fn with_changes(mut self, changes: Vec<ParameterChange>) -> Self {
        self.parameter_changes = changes;
        self
    }

    pub fn with_reasoning(mut self, reasoning: &str) -> Self {
        self.reasoning = reasoning.to_string();
        self
    }
}

/// Type of action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    /// Added a new effect
    Add,
    /// Modified an existing effect
    Modify,
    /// Removed an effect
    Remove,
    /// Enabled/disabled an effect
    Toggle,
    /// Reordered effects
    Reorder,
    /// Neural processing
    NeuralProcess,
    /// Undo
    Undo,
    /// Redo
    Redo,
}

/// Reference to an effect in the chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectRef {
    /// Effect ID
    pub id: String,

    /// Effect type (eq, compressor, reverb, etc.)
    pub effect_type: String,

    /// Display name
    pub display_name: String,

    /// Position in chain (0-indexed)
    pub chain_index: usize,
}

/// A parameter change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterChange {
    /// Effect name
    pub effect_name: String,

    /// Parameter name
    pub param: String,

    /// Old value
    pub old_value: serde_json::Value,

    /// New value
    pub new_value: serde_json::Value,
}

/// User preferences learned from conversation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserPreferences {
    /// Whether user prefers DSP first
    pub prefers_dsp_first: Option<bool>,

    /// Compression style preference
    pub compression_preference: Option<String>,

    /// Typical genre
    pub typical_genre: Option<String>,

    /// Custom preferences
    #[serde(flatten)]
    pub custom: HashMap<String, serde_json::Value>,
}

/// Full conversation context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationContext {
    /// Session ID
    pub session_id: String,

    /// Full conversation history
    pub messages: Vec<Message>,

    /// Recent actions taken by agent
    pub recent_actions: Vec<AgentAction>,

    /// User preferences learned from conversation
    pub user_preferences: UserPreferences,

    /// Current effect focus (which effect we're talking about)
    pub effect_focus: Option<EffectFocus>,

    /// Message index counter
    message_index: usize,
}

impl ConversationContext {
    pub fn new() -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            messages: Vec::new(),
            recent_actions: Vec::new(),
            user_preferences: UserPreferences::default(),
            effect_focus: None,
            message_index: 0,
        }
    }

    pub fn with_session_id(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            ..Self::new()
        }
    }

    /// Add a user message
    pub fn add_user_message(&mut self, content: &str) -> &Message {
        let msg = Message::user(content);
        self.messages.push(msg);
        self.message_index += 1;
        self.messages.last().unwrap()
    }

    /// Add an agent message
    pub fn add_agent_message(&mut self, content: &str) -> &Message {
        let msg = Message::agent(content);
        self.messages.push(msg);
        self.message_index += 1;
        self.messages.last().unwrap()
    }

    /// Add an agent message with associated action
    pub fn add_agent_message_with_action(&mut self, content: &str, action: AgentAction) -> &Message {
        let action_id = action.id.clone();

        // Update effect focus if action affected an effect
        if let Some(ref effect) = action.affected_effect {
            self.effect_focus = Some(EffectFocus {
                effect_id: effect.id.clone(),
                effect_type: effect.effect_type.clone(),
                since_message_index: self.message_index,
            });
        }

        self.recent_actions.push(action);

        let msg = Message::agent(content).with_action(&action_id);
        self.messages.push(msg);
        self.message_index += 1;
        self.messages.last().unwrap()
    }

    /// Get the last action taken
    pub fn last_action(&self) -> Option<&AgentAction> {
        self.recent_actions.last()
    }

    /// Get effects mentioned in conversation
    pub fn effects_mentioned(&self) -> HashSet<String> {
        self.recent_actions
            .iter()
            .filter_map(|a| a.affected_effect.as_ref())
            .map(|e| e.effect_type.clone())
            .collect()
    }

    /// Get the current message index
    pub fn current_index(&self) -> usize {
        self.message_index
    }

    /// Clear conversation but keep preferences
    pub fn clear_conversation(&mut self) {
        self.messages.clear();
        self.recent_actions.clear();
        self.effect_focus = None;
        self.message_index = 0;
    }
}

impl Default for ConversationContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracks which effect the conversation is currently about
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectFocus {
    /// Focused effect ID
    pub effect_id: String,

    /// Focused effect type
    pub effect_type: String,

    /// Since which message index
    pub since_message_index: usize,
}

impl EffectFocus {
    /// Check if we should modify the focused effect or add new
    pub fn should_modify_vs_add(&self, prompt: &str, effect_type: &str) -> ModifyOrAdd {
        const MODIFICATION_SIGNALS: &[&str] = &[
            "more",
            "less",
            "increase",
            "decrease",
            "adjust",
            "too much",
            "not enough",
            "back off",
            "push it",
            "tweak",
            "change",
            "reduce",
            "boost",
        ];

        let prompt_lower = prompt.to_lowercase();

        // If same effect type and has modification signals
        if self.effect_type == effect_type {
            if MODIFICATION_SIGNALS
                .iter()
                .any(|sig| prompt_lower.contains(sig))
            {
                return ModifyOrAdd::Modify;
            }
        }

        ModifyOrAdd::Add
    }
}

/// Whether to modify existing effect or add new
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifyOrAdd {
    Modify,
    Add,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_context_new() {
        let ctx = ConversationContext::new();
        assert!(ctx.messages.is_empty());
        assert!(ctx.recent_actions.is_empty());
    }

    #[test]
    fn test_add_messages() {
        let mut ctx = ConversationContext::new();

        ctx.add_user_message("add compression");
        ctx.add_agent_message("Added compressor");

        assert_eq!(ctx.messages.len(), 2);
        assert_eq!(ctx.messages[0].role, MessageRole::User);
        assert_eq!(ctx.messages[1].role, MessageRole::Agent);
    }

    #[test]
    fn test_add_action() {
        let mut ctx = ConversationContext::new();

        let effect = EffectRef {
            id: "comp-1".to_string(),
            effect_type: "compressor".to_string(),
            display_name: "Compressor".to_string(),
            chain_index: 0,
        };

        let action = AgentAction::new(ActionType::Add, ToolType::Dsp, "Added compressor")
            .with_effect(effect);

        ctx.add_agent_message_with_action("Added compressor with gentle settings", action);

        assert!(ctx.last_action().is_some());
        assert!(ctx.effect_focus.is_some());
        assert_eq!(ctx.effect_focus.as_ref().unwrap().effect_type, "compressor");
    }

    #[test]
    fn test_effect_focus_modify_vs_add() {
        let focus = EffectFocus {
            effect_id: "comp-1".to_string(),
            effect_type: "compressor".to_string(),
            since_message_index: 0,
        };

        // Same type with modification signal
        assert_eq!(
            focus.should_modify_vs_add("make it more aggressive", "compressor"),
            ModifyOrAdd::Modify
        );

        // Same type with add signal
        assert_eq!(
            focus.should_modify_vs_add("add another compressor", "compressor"),
            ModifyOrAdd::Add
        );

        // Different type
        assert_eq!(
            focus.should_modify_vs_add("add some reverb", "reverb"),
            ModifyOrAdd::Add
        );
    }

    #[test]
    fn test_effects_mentioned() {
        let mut ctx = ConversationContext::new();

        let action1 = AgentAction::new(ActionType::Add, ToolType::Dsp, "Added EQ").with_effect(
            EffectRef {
                id: "eq-1".to_string(),
                effect_type: "eq".to_string(),
                display_name: "EQ".to_string(),
                chain_index: 0,
            },
        );

        let action2 =
            AgentAction::new(ActionType::Add, ToolType::Dsp, "Added compressor").with_effect(
                EffectRef {
                    id: "comp-1".to_string(),
                    effect_type: "compressor".to_string(),
                    display_name: "Compressor".to_string(),
                    chain_index: 1,
                },
            );

        ctx.add_agent_message_with_action("Added EQ", action1);
        ctx.add_agent_message_with_action("Added compressor", action2);

        let mentioned = ctx.effects_mentioned();
        assert!(mentioned.contains("eq"));
        assert!(mentioned.contains("compressor"));
    }
}
