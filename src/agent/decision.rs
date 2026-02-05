//! Agent decision logic - Tool selection and confidence scoring
//!
//! Implements §5.7 and §6 from the spec.

use serde::{Deserialize, Serialize};

use super::intent::Intent;

/// Type of tool the agent can select
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolType {
    /// DSP effects (Layer 2) - fast, deterministic
    Dsp,
    /// Neural models (Layer 1) - transformative, slower
    Neural,
    /// Both tools needed
    Both,
    /// Need to ask for clarification
    AskClarification,
}

/// Result of tool decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDecision {
    /// Selected tool type
    pub tool: ToolType,

    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,

    /// Recommended effects/models
    pub recommendations: Vec<String>,

    /// Reasoning for the decision
    pub reasoning: String,

    /// Whether to ask for clarification
    pub ask_clarification: bool,
}

impl ToolDecision {
    pub fn new(tool: ToolType, confidence: f32) -> Self {
        Self {
            tool,
            confidence,
            recommendations: Vec::new(),
            reasoning: String::new(),
            ask_clarification: false,
        }
    }

    pub fn with_recommendations(mut self, recs: Vec<String>) -> Self {
        self.recommendations = recs;
        self
    }

    pub fn with_reasoning(mut self, reasoning: &str) -> Self {
        self.reasoning = reasoning.to_string();
        self
    }

    pub fn needs_clarification(mut self) -> Self {
        self.ask_clarification = true;
        self
    }
}

/// Agent response to a user request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    /// What action was taken
    pub action: AgentAction,

    /// Message to show the user
    pub message: String,

    /// Decision details
    pub decision: Option<ToolDecision>,

    /// Changes that were made (for "executed" action)
    pub changes: Vec<String>,
}

/// Type of action the agent took
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentAction {
    /// High confidence - executed automatically
    Executed,
    /// Medium confidence - proposing changes
    Propose,
    /// Low confidence - asking for clarification
    Clarify,
    /// Very low confidence - admitting uncertainty
    Uncertain,
}

/// Confidence thresholds per spec §6.3
pub mod confidence {
    /// Just do it, report what was done
    pub const AUTO_EXECUTE: f32 = 0.80;
    /// "I'm going to add X, sound good?"
    pub const SUGGEST_FIRST: f32 = 0.60;
    /// "Could you tell me more about..."
    pub const ASK_CLARIFICATION: f32 = 0.40;
    /// "I'm not sure what you mean..."
    pub const REFUSE_GRACEFULLY: f32 = 0.20;
}

/// The AI Agent for audio processing decisions
pub struct Agent {
    // Future: conversation context, user preferences, etc.
}

impl Agent {
    pub fn new() -> Self {
        Self {}
    }

    /// Main entry point: decide what tool to use for a prompt
    pub fn decide_tool(&self, prompt: &str) -> ToolDecision {
        let intent = Intent::analyze(prompt);
        self.decide_from_intent(&intent)
    }

    /// Decide based on analyzed intent
    pub fn decide_from_intent(&self, intent: &Intent) -> ToolDecision {
        // Step 1: Check for explicit tool requests
        if intent.explicit_dsp_request {
            return ToolDecision::new(ToolType::Dsp, 0.95)
                .with_reasoning("User explicitly requested DSP tool");
        }

        if intent.explicit_neural_request {
            return ToolDecision::new(ToolType::Neural, 0.95)
                .with_reasoning("User explicitly requested neural/AI processing");
        }

        // Step 2: Check for neural-only capabilities
        if self.requires_neural(intent) {
            return ToolDecision::new(ToolType::Neural, 0.85).with_reasoning(
                "Request requires semantic understanding or holistic transformation",
            );
        }

        // Step 3: Check if DSP can handle it
        if self.dsp_can_handle(intent) {
            return ToolDecision::new(ToolType::Dsp, 0.80)
                .with_reasoning("Standard mixing task - DSP is fast and tweakable");
        }

        // Step 4: Check for truly vague/ambiguous requests (per spec §6.1)
        if self.is_truly_ambiguous(intent) {
            return ToolDecision::new(ToolType::AskClarification, 0.20)
                .with_reasoning("Request is too vague - need more details")
                .needs_clarification();
        }

        // Step 5: Complex requests might need both
        if intent.is_complex {
            return ToolDecision::new(ToolType::Both, 0.70)
                .with_reasoning("Complex request may benefit from both tools");
        }

        // Step 6: Ambiguous - prefer DSP (faster, more control)
        ToolDecision::new(ToolType::Dsp, 0.50)
            .with_reasoning("Ambiguous request - defaulting to DSP for speed and control")
            .needs_clarification()
    }

    /// Check if the request is truly ambiguous (no clear intent)
    fn is_truly_ambiguous(&self, intent: &Intent) -> bool {
        const VAGUE_ONLY: &[&str] = &["better", "improve", "good", "nice", "fix it"];

        // If prompt ONLY contains vague words without any specific indicators
        let has_vague = VAGUE_ONLY
            .iter()
            .any(|v| intent.prompt_lower.contains(v));

        // But doesn't have specific effect mentions or clear direction
        let has_specific = !intent.mentioned_effects.is_empty()
            || !intent.extracted_params.is_empty()
            || self.requires_neural(intent)
            || self.dsp_can_handle(intent);

        has_vague && !has_specific
    }

    /// Check if the request fundamentally requires neural processing
    fn requires_neural(&self, intent: &Intent) -> bool {
        const NEURAL_INDICATORS: &[&str] = &[
            "sound like",
            "as if recorded",
            "make it sound like a",
            "vintage",
            "old recording",
            "remove noise",
            "fix the clipping",
            "restore",
            "reimagine",
            "transform into",
            "in the style of",
            "denoise",
            "style transfer",
        ];

        let prompt_lower = intent.prompt_lower.as_str();
        NEURAL_INDICATORS
            .iter()
            .any(|indicator| prompt_lower.contains(indicator))
    }

    /// Check if standard DSP effects can achieve the goal
    fn dsp_can_handle(&self, intent: &Intent) -> bool {
        const DSP_INDICATORS: &[&str] = &[
            "eq",
            "equalize",
            "equalizer",
            "compress",
            "compressor",
            "compression",
            "reverb",
            "echo",
            "delay",
            "louder",
            "quieter",
            "volume",
            "gain",
            "bass",
            "treble",
            "mids",
            "highs",
            "lows",
            "brighter",
            "darker",
            "punchier",
            "punch",
            "limit",
            "limiter",
            "gate",
            "noise gate",
        ];

        let prompt_lower = intent.prompt_lower.as_str();
        DSP_INDICATORS
            .iter()
            .any(|indicator| prompt_lower.contains(indicator))
    }

    /// Handle confidence level and generate appropriate response
    pub fn handle_decision(&self, decision: &ToolDecision) -> AgentResponse {
        if decision.confidence >= confidence::AUTO_EXECUTE {
            AgentResponse {
                action: AgentAction::Executed,
                message: format!("Done! {}", decision.reasoning),
                decision: Some(decision.clone()),
                changes: decision.recommendations.clone(),
            }
        } else if decision.confidence >= confidence::SUGGEST_FIRST {
            AgentResponse {
                action: AgentAction::Propose,
                message: format!(
                    "I'm thinking of using {} tools. Should I go ahead?",
                    match decision.tool {
                        ToolType::Dsp => "DSP",
                        ToolType::Neural => "neural/AI",
                        ToolType::Both => "both DSP and neural",
                        ToolType::AskClarification => "clarification needed",
                    }
                ),
                decision: Some(decision.clone()),
                changes: Vec::new(),
            }
        } else if decision.confidence >= confidence::ASK_CLARIFICATION {
            AgentResponse {
                action: AgentAction::Clarify,
                message: "Could you tell me more about what you're looking for?".to_string(),
                decision: Some(decision.clone()),
                changes: Vec::new(),
            }
        } else {
            AgentResponse {
                action: AgentAction::Uncertain,
                message: "I'm not quite sure what you're looking for. Could you describe what you want to achieve in different words?".to_string(),
                decision: Some(decision.clone()),
                changes: Vec::new(),
            }
        }
    }
}

impl Default for Agent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explicit_dsp_request() {
        let agent = Agent::new();
        let decision = agent.decide_tool("add an EQ");
        assert_eq!(decision.tool, ToolType::Dsp);
        assert!(decision.confidence >= 0.80);
    }

    #[test]
    fn test_explicit_neural_request() {
        let agent = Agent::new();
        let decision = agent.decide_tool("make it sound like a vintage recording");
        assert_eq!(decision.tool, ToolType::Neural);
    }

    #[test]
    fn test_standard_dsp_task() {
        let agent = Agent::new();
        let decision = agent.decide_tool("make it louder");
        assert_eq!(decision.tool, ToolType::Dsp);
    }

    #[test]
    fn test_ambiguous_request() {
        let agent = Agent::new();
        let decision = agent.decide_tool("make it better");
        assert!(decision.confidence < confidence::AUTO_EXECUTE);
        assert!(decision.ask_clarification);
    }
}
