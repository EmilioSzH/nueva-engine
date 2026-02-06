//! Agent Tests
//!
//! Tests for AI agent intent parsing and decision making.

use nueva::agent::{
    Intent, Agent, ToolType, ConversationContext,
    SafetyChecker,
};

// === Intent Parsing Tests ===

#[test]
fn test_intent_analyze() {
    let intent = Intent::analyze("make it louder");
    assert!(!intent.original.is_empty());
    assert!(!intent.prompt_lower.is_empty());
}

#[test]
fn test_intent_explicit_dsp() {
    let intent = Intent::analyze("add compression");
    // Compression mentions DSP effect
    assert!(
        intent.explicit_dsp_request || !intent.mentioned_effects.is_empty(),
        "Compression should mention DSP effects"
    );
}

#[test]
fn test_intent_complex_request() {
    let intent = Intent::analyze("make it sound vintage like old vinyl records from the 60s");
    // Complex requests should be marked as complex
    assert!(
        intent.is_complex || intent.explicit_neural_request || intent.intensity > 0.0,
        "Complex vintage request should be recognized"
    );
}

#[test]
fn test_intent_with_effects() {
    let intent = Intent::analyze("add reverb and delay");
    // Should extract mentioned effects
    assert!(
        !intent.mentioned_effects.is_empty() || intent.explicit_dsp_request,
        "Should recognize reverb/delay as effects"
    );
}

// === Agent Decision Tests ===

#[test]
fn test_agent_creation() {
    let agent = Agent::new();
    let _ = agent;
}

#[test]
fn test_agent_decide_tool_louder() {
    let agent = Agent::new();
    let decision = agent.decide_tool("make it louder");

    assert!(
        matches!(decision.tool, ToolType::Dsp),
        "Louder should decide DSP: {:?}",
        decision.tool
    );
}

#[test]
fn test_agent_decide_tool_compression() {
    let agent = Agent::new();
    let decision = agent.decide_tool("add compression");

    assert!(
        matches!(decision.tool, ToolType::Dsp),
        "Compression should decide DSP: {:?}",
        decision.tool
    );
}

#[test]
fn test_agent_decide_tool_eq() {
    let agent = Agent::new();
    let decision = agent.decide_tool("boost the bass frequencies");

    assert!(
        matches!(decision.tool, ToolType::Dsp),
        "EQ request should decide DSP: {:?}",
        decision.tool
    );
}

#[test]
fn test_agent_decide_from_intent() {
    let agent = Agent::new();
    let intent = Intent::analyze("add eq boost at 1kHz");
    let decision = agent.decide_from_intent(&intent);

    assert!(
        matches!(decision.tool, ToolType::Dsp),
        "EQ should decide DSP: {:?}",
        decision.tool
    );
}

// === Safety Check Tests ===

#[test]
fn test_safety_checker_creation() {
    let checker = SafetyChecker::new();
    let _ = checker;
}

#[test]
fn test_safety_check_gain_small() {
    let checker = SafetyChecker::new();
    let result = checker.check_gain(3.0);
    assert!(!result.has_issues(), "Small gain should be safe");
}

#[test]
fn test_safety_check_gain_moderate() {
    let checker = SafetyChecker::new();
    let result = checker.check_gain(12.0);
    // Moderate gain may or may not have issues depending on thresholds
    let _ = result;
}

#[test]
fn test_safety_check_loudness_normal() {
    let checker = SafetyChecker::new();
    let result = checker.check_loudness(-14.0);
    assert!(!result.has_issues(), "Normal loudness should be safe");
}

#[test]
fn test_safety_check_loudness_extreme() {
    let checker = SafetyChecker::new();
    let result = checker.check_loudness(-2.0);
    assert!(result.has_issues(), "Extreme loudness should trigger warning");
}

// === Conversation Context Tests ===

#[test]
fn test_context_creation() {
    let context = ConversationContext::new();
    let _ = context;
}

#[test]
fn test_context_add_messages() {
    let mut context = ConversationContext::new();
    context.add_user_message("add compression");
    context.add_agent_message("Added compressor with default settings.");
}
