//! Reference resolution for conversation context
//!
//! Resolves ambiguous references like "that", "it", "the EQ"
//! Implements §7.2 from the spec.

use super::context::{ConversationContext, EffectRef};

/// Known effect types for reference resolution
const EFFECT_TYPES: &[&str] = &[
    "eq",
    "equalizer",
    "compressor",
    "compression",
    "reverb",
    "delay",
    "echo",
    "gate",
    "limiter",
    "saturation",
    "distortion",
    "chorus",
    "flanger",
    "phaser",
];

/// Canonical effect type mapping
fn canonicalize_effect_type(effect_type: &str) -> &'static str {
    match effect_type {
        "equalizer" => "eq",
        "compression" => "compressor",
        "echo" => "delay",
        "distortion" => "saturation",
        _ => {
            // Return static str for known types
            for &t in EFFECT_TYPES {
                if t == effect_type {
                    return t;
                }
            }
            "unknown"
        }
    }
}

/// Result of reference resolution
#[derive(Debug, Clone)]
pub enum ResolvedReference {
    /// Resolved to a specific effect
    Effect(EffectRef),

    /// Resolved to "undo last action"
    UndoLast,

    /// Resolved to "redo"
    Redo,

    /// Resolved to "repeat last action"
    RepeatLast,

    /// Resolved to "explain last action"
    ExplainLast,

    /// Could not resolve
    Unresolved,
}

/// Resolve a reference in the user's prompt
///
/// # Arguments
/// * `reference` - The reference text (e.g., "the EQ", "that", "it")
/// * `context` - Current conversation context
/// * `dsp_chain` - Current DSP effect chain (effect refs in order)
///
/// # Returns
/// Resolved reference or Unresolved
pub fn resolve_reference(
    reference: &str,
    context: &ConversationContext,
    dsp_chain: &[EffectRef],
) -> ResolvedReference {
    let ref_lower = reference.to_lowercase();

    // Check for action references first
    if is_undo_reference(&ref_lower) {
        return ResolvedReference::UndoLast;
    }

    if is_redo_reference(&ref_lower) {
        return ResolvedReference::Redo;
    }

    if is_repeat_reference(&ref_lower) {
        return ResolvedReference::RepeatLast;
    }

    if is_explain_reference(&ref_lower) {
        return ResolvedReference::ExplainLast;
    }

    // Check for specific effect type reference ("the EQ", "that compressor")
    for &effect_type in EFFECT_TYPES {
        if ref_lower.contains(effect_type) {
            let canonical = canonicalize_effect_type(effect_type);
            if let Some(effect) = find_most_recent_effect_by_type(context, dsp_chain, canonical) {
                return ResolvedReference::Effect(effect);
            }
        }
    }

    // Check for ordinal reference ("first effect", "last one")
    if ref_lower.contains("first") {
        if let Some(effect) = dsp_chain.first() {
            return ResolvedReference::Effect(effect.clone());
        }
    }

    if ref_lower.contains("last") {
        if let Some(effect) = dsp_chain.last() {
            return ResolvedReference::Effect(effect.clone());
        }
    }

    // Check for generic reference ("it", "that", "this")
    if is_generic_reference(&ref_lower) {
        if let Some(action) = context.last_action() {
            if let Some(effect) = &action.affected_effect {
                return ResolvedReference::Effect(effect.clone());
            }
        }
    }

    ResolvedReference::Unresolved
}

/// Check if reference is about undoing
fn is_undo_reference(ref_lower: &str) -> bool {
    ref_lower.contains("undo") || ref_lower == "go back" || ref_lower == "revert"
}

/// Check if reference is about redoing
fn is_redo_reference(ref_lower: &str) -> bool {
    ref_lower.contains("redo") || ref_lower == "go forward"
}

/// Check if reference is about repeating
fn is_repeat_reference(ref_lower: &str) -> bool {
    ref_lower.contains("again")
        || ref_lower.contains("repeat")
        || ref_lower == "do that again"
        || ref_lower == "same thing"
}

/// Check if reference is asking for explanation
fn is_explain_reference(ref_lower: &str) -> bool {
    ref_lower.contains("what did you do")
        || ref_lower.contains("explain")
        || ref_lower.contains("what happened")
        || ref_lower.contains("what was that")
}

/// Check if reference is generic ("it", "that", "this")
fn is_generic_reference(ref_lower: &str) -> bool {
    // Check for standalone pronouns
    let words: Vec<&str> = ref_lower.split_whitespace().collect();
    words.contains(&"it") || words.contains(&"that") || words.contains(&"this")
}

/// Find the most recent effect of a given type
fn find_most_recent_effect_by_type(
    context: &ConversationContext,
    dsp_chain: &[EffectRef],
    effect_type: &str,
) -> Option<EffectRef> {
    // First check recent actions for this effect type
    for action in context.recent_actions.iter().rev() {
        if let Some(ref effect) = action.affected_effect {
            if effect.effect_type == effect_type {
                // Verify it's still in the chain
                if dsp_chain.iter().any(|e| e.id == effect.id) {
                    return Some(effect.clone());
                }
            }
        }
    }

    // Fall back to finding any effect of this type in the chain
    dsp_chain
        .iter()
        .rev()
        .find(|e| e.effect_type == effect_type)
        .cloned()
}

/// Parse intensity modifiers from reference
/// Returns (base_reference, intensity_modifier)
pub fn parse_intensity_modifier(prompt: &str) -> (String, IntensityModifier) {
    let prompt_lower = prompt.to_lowercase();

    // Check for "more of that" style
    if prompt_lower.contains("more of") {
        let base = prompt_lower.replace("more of", "").trim().to_string();
        return (base, IntensityModifier::Increase(0.3));
    }

    if prompt_lower.contains("less of") {
        let base = prompt_lower.replace("less of", "").trim().to_string();
        return (base, IntensityModifier::Decrease(0.3));
    }

    if prompt_lower.contains("much more") {
        return (prompt_lower, IntensityModifier::Increase(0.5));
    }

    if prompt_lower.contains("much less") {
        return (prompt_lower, IntensityModifier::Decrease(0.5));
    }

    (prompt_lower, IntensityModifier::None)
}

/// Intensity modification direction
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IntensityModifier {
    /// No modification
    None,
    /// Increase by factor (0.0-1.0)
    Increase(f32),
    /// Decrease by factor (0.0-1.0)
    Decrease(f32),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::context::{AgentAction, ActionType};
    use crate::agent::decision::ToolType;

    fn make_effect(id: &str, effect_type: &str, index: usize) -> EffectRef {
        EffectRef {
            id: id.to_string(),
            effect_type: effect_type.to_string(),
            display_name: effect_type.to_string(),
            chain_index: index,
        }
    }

    #[test]
    fn test_resolve_specific_effect_type() {
        let mut ctx = ConversationContext::new();
        let dsp_chain = vec![
            make_effect("eq-1", "eq", 0),
            make_effect("comp-1", "compressor", 1),
        ];

        // Add action affecting the EQ
        let action =
            AgentAction::new(ActionType::Add, ToolType::Dsp, "Added EQ").with_effect(dsp_chain[0].clone());
        ctx.add_agent_message_with_action("Added EQ", action);

        let result = resolve_reference("the EQ", &ctx, &dsp_chain);
        match result {
            ResolvedReference::Effect(e) => assert_eq!(e.effect_type, "eq"),
            _ => panic!("Expected Effect resolution"),
        }
    }

    #[test]
    fn test_resolve_first_last() {
        let ctx = ConversationContext::new();
        let dsp_chain = vec![
            make_effect("eq-1", "eq", 0),
            make_effect("comp-1", "compressor", 1),
            make_effect("reverb-1", "reverb", 2),
        ];

        let first = resolve_reference("the first effect", &ctx, &dsp_chain);
        match first {
            ResolvedReference::Effect(e) => assert_eq!(e.id, "eq-1"),
            _ => panic!("Expected first effect"),
        }

        let last = resolve_reference("the last one", &ctx, &dsp_chain);
        match last {
            ResolvedReference::Effect(e) => assert_eq!(e.id, "reverb-1"),
            _ => panic!("Expected last effect"),
        }
    }

    #[test]
    fn test_resolve_generic_reference() {
        let mut ctx = ConversationContext::new();
        let dsp_chain = vec![make_effect("comp-1", "compressor", 0)];

        let action = AgentAction::new(ActionType::Add, ToolType::Dsp, "Added compressor")
            .with_effect(dsp_chain[0].clone());
        ctx.add_agent_message_with_action("Added compressor", action);

        let result = resolve_reference("adjust it", &ctx, &dsp_chain);
        match result {
            ResolvedReference::Effect(e) => assert_eq!(e.effect_type, "compressor"),
            _ => panic!("Expected effect from 'it' reference"),
        }
    }

    #[test]
    fn test_resolve_undo() {
        let ctx = ConversationContext::new();
        let dsp_chain = vec![];

        let result = resolve_reference("undo that", &ctx, &dsp_chain);
        assert!(matches!(result, ResolvedReference::UndoLast));
    }

    #[test]
    fn test_resolve_explain() {
        let ctx = ConversationContext::new();
        let dsp_chain = vec![];

        let result = resolve_reference("what did you do?", &ctx, &dsp_chain);
        assert!(matches!(result, ResolvedReference::ExplainLast));
    }

    #[test]
    fn test_parse_intensity_modifier() {
        let (_, modifier) = parse_intensity_modifier("more of that compression");
        assert!(matches!(modifier, IntensityModifier::Increase(_)));

        let (_, modifier) = parse_intensity_modifier("less of that");
        assert!(matches!(modifier, IntensityModifier::Decrease(_)));

        let (_, modifier) = parse_intensity_modifier("adjust the EQ");
        assert!(matches!(modifier, IntensityModifier::None));
    }
}
