//! Explanation generation for agent actions
//!
//! Users can always ask what the agent did.
//! Implements §7.5 from the spec.

use super::context::{AgentAction, ConversationContext, EffectRef, ParameterChange};
use super::decision::ToolType;
use std::collections::HashMap;

/// Explain the last action taken by the agent
pub fn explain_last_action(context: &ConversationContext) -> String {
    let action = match context.last_action() {
        Some(a) => a,
        None => return "I haven't made any changes yet.".to_string(),
    };

    let mut explanation = format!("I {}.\n\n", action.description);

    match action.tool {
        ToolType::Dsp => {
            if !action.parameter_changes.is_empty() {
                explanation.push_str("Here's what changed:\n");
                for change in &action.parameter_changes {
                    explanation.push_str(&format!(
                        "  - {}: {} went from {} to {}\n",
                        change.effect_name,
                        change.param,
                        format_value(&change.old_value),
                        format_value(&change.new_value)
                    ));
                }
            }

            if let Some(ref effect) = action.affected_effect {
                explanation.push_str(&format!("\nEffect: {} ({})\n", effect.display_name, effect.id));
            }
        }
        ToolType::Neural => {
            if let Some(ref model) = action.model_name {
                explanation.push_str(&format!("I used the {} model to process your audio.\n", model));
            }

            if let Some(ref params) = action.model_params {
                explanation.push_str(&format!("Settings: {}\n", format_model_params(params)));
            }
        }
        ToolType::Both => {
            explanation.push_str("This used both DSP effects and neural processing.\n");

            if let Some(ref model) = action.model_name {
                explanation.push_str(&format!("Neural model: {}\n", model));
            }

            if !action.parameter_changes.is_empty() {
                explanation.push_str("DSP changes:\n");
                for change in &action.parameter_changes {
                    explanation.push_str(&format!(
                        "  - {}: {} = {}\n",
                        change.effect_name,
                        change.param,
                        format_value(&change.new_value)
                    ));
                }
            }
        }
        ToolType::AskClarification => {
            explanation = "I asked for clarification because the request was ambiguous.".to_string();
        }
    }

    if !action.reasoning.is_empty() {
        explanation.push_str(&format!("\nI did this because: {}", action.reasoning));
    }

    explanation
}

/// Explain the full effect chain
pub fn explain_full_chain(dsp_chain: &[EffectRef], effect_params: &HashMap<String, HashMap<String, serde_json::Value>>) -> String {
    if dsp_chain.is_empty() {
        return "No effects are currently applied. The audio is passing through clean.".to_string();
    }

    let mut explanation = format!("Here's your current effect chain ({} effects):\n\n", dsp_chain.len());

    for (i, effect) in dsp_chain.iter().enumerate() {
        explanation.push_str(&format!("{}. {}", i + 1, effect.display_name));

        // Check if we have params for this effect
        if let Some(params) = effect_params.get(&effect.id) {
            let param_str = describe_effect_settings(&effect.effect_type, params);
            if !param_str.is_empty() {
                explanation.push_str(&format!("\n   {}", param_str));
            }
        }

        explanation.push('\n');
    }

    explanation
}

/// Format a JSON value for display
fn format_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.fract() == 0.0 {
                    format!("{}", f as i64)
                } else {
                    format!("{:.2}", f)
                }
            } else {
                n.to_string()
            }
        }
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => if *b { "on" } else { "off" }.to_string(),
        _ => value.to_string(),
    }
}

/// Format model parameters for display
fn format_model_params(params: &HashMap<String, serde_json::Value>) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}: {}", k, format_value(v)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Describe effect settings in human-readable form
fn describe_effect_settings(
    effect_type: &str,
    params: &HashMap<String, serde_json::Value>,
) -> String {
    match effect_type {
        "eq" | "equalizer" => describe_eq_settings(params),
        "compressor" | "compression" => describe_compressor_settings(params),
        "reverb" => describe_reverb_settings(params),
        "delay" | "echo" => describe_delay_settings(params),
        "limiter" => describe_limiter_settings(params),
        "gate" => describe_gate_settings(params),
        _ => format_model_params(params),
    }
}

fn describe_eq_settings(params: &HashMap<String, serde_json::Value>) -> String {
    let mut parts = Vec::new();

    if let Some(bands) = params.get("bands").and_then(|v| v.as_array()) {
        for band in bands {
            if let (Some(freq), Some(gain)) = (
                band.get("freq").and_then(|v| v.as_f64()),
                band.get("gain").and_then(|v| v.as_f64()),
            ) {
                let sign = if gain >= 0.0 { "+" } else { "" };
                parts.push(format!("{}{}dB @ {}Hz", sign, gain, freq as i32));
            }
        }
    }

    if parts.is_empty() {
        "default settings".to_string()
    } else {
        parts.join(", ")
    }
}

fn describe_compressor_settings(params: &HashMap<String, serde_json::Value>) -> String {
    let threshold = params
        .get("threshold")
        .and_then(|v| v.as_f64())
        .map(|v| format!("threshold {}dB", v))
        .unwrap_or_default();

    let ratio = params
        .get("ratio")
        .and_then(|v| v.as_f64())
        .map(|v| format!("ratio {}:1", v))
        .unwrap_or_default();

    let attack = params
        .get("attack_ms")
        .and_then(|v| v.as_f64())
        .map(|v| format!("attack {}ms", v))
        .unwrap_or_default();

    let release = params
        .get("release_ms")
        .and_then(|v| v.as_f64())
        .map(|v| format!("release {}ms", v))
        .unwrap_or_default();

    vec![threshold, ratio, attack, release]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn describe_reverb_settings(params: &HashMap<String, serde_json::Value>) -> String {
    let room_size = params
        .get("room_size")
        .and_then(|v| v.as_str())
        .unwrap_or("medium");

    let mix = params
        .get("mix")
        .or_else(|| params.get("wet"))
        .and_then(|v| v.as_f64())
        .map(|v| format!("{}% wet", (v * 100.0) as i32))
        .unwrap_or_default();

    if mix.is_empty() {
        format!("{} room", room_size)
    } else {
        format!("{} room, {}", room_size, mix)
    }
}

fn describe_delay_settings(params: &HashMap<String, serde_json::Value>) -> String {
    let time = params
        .get("time_ms")
        .and_then(|v| v.as_f64())
        .map(|v| format!("{}ms", v))
        .unwrap_or_default();

    let feedback = params
        .get("feedback")
        .and_then(|v| v.as_f64())
        .map(|v| format!("{}% feedback", (v * 100.0) as i32))
        .unwrap_or_default();

    vec![time, feedback]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn describe_limiter_settings(params: &HashMap<String, serde_json::Value>) -> String {
    let ceiling = params
        .get("ceiling")
        .and_then(|v| v.as_f64())
        .map(|v| format!("ceiling {}dB", v))
        .unwrap_or_else(|| "ceiling -0.1dB".to_string());

    ceiling
}

fn describe_gate_settings(params: &HashMap<String, serde_json::Value>) -> String {
    let threshold = params
        .get("threshold")
        .and_then(|v| v.as_f64())
        .map(|v| format!("threshold {}dB", v))
        .unwrap_or_default();

    threshold
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::context::{ActionType, AgentAction};

    #[test]
    fn test_explain_no_action() {
        let ctx = ConversationContext::default();
        let explanation = explain_last_action(&ctx);
        assert!(explanation.contains("haven't made any changes"));
    }

    #[test]
    fn test_explain_dsp_action() {
        let mut ctx = ConversationContext::default();

        let effect = EffectRef {
            id: "eq-1".to_string(),
            effect_type: "eq".to_string(),
            display_name: "Parametric EQ".to_string(),
            chain_index: 0,
        };

        let change = ParameterChange {
            effect_name: "EQ".to_string(),
            param: "band1_gain".to_string(),
            old_value: serde_json::json!(0.0),
            new_value: serde_json::json!(3.0),
        };

        let action = AgentAction::new(ActionType::Modify, ToolType::Dsp, "boosted the low mids")
            .with_effect(effect)
            .with_changes(vec![change])
            .with_reasoning("User wanted more warmth");

        ctx.add_agent_message_with_action("Boosted low mids", action);

        let explanation = explain_last_action(&ctx);
        assert!(explanation.contains("boosted the low mids"));
        assert!(explanation.contains("went from 0 to 3"));
        assert!(explanation.contains("more warmth"));
    }

    #[test]
    fn test_explain_neural_action() {
        let mut ctx = ConversationContext::default();

        let mut params = HashMap::new();
        params.insert("style_preset".to_string(), serde_json::json!("tape_warmth"));
        params.insert("intensity".to_string(), serde_json::json!(0.6));

        let action = AgentAction::new(
            ActionType::NeuralProcess,
            ToolType::Neural,
            "applied vintage tape warmth",
        )
        .with_model("style-transfer", params);

        ctx.add_agent_message_with_action("Applied style transfer", action);

        let explanation = explain_last_action(&ctx);
        assert!(explanation.contains("style-transfer"));
        assert!(explanation.contains("tape_warmth"));
    }

    #[test]
    fn test_explain_full_chain() {
        let chain = vec![
            EffectRef {
                id: "eq-1".to_string(),
                effect_type: "eq".to_string(),
                display_name: "Parametric EQ".to_string(),
                chain_index: 0,
            },
            EffectRef {
                id: "comp-1".to_string(),
                effect_type: "compressor".to_string(),
                display_name: "Compressor".to_string(),
                chain_index: 1,
            },
        ];

        let mut params = HashMap::new();
        let mut comp_params = HashMap::new();
        comp_params.insert("threshold".to_string(), serde_json::json!(-18.0));
        comp_params.insert("ratio".to_string(), serde_json::json!(4.0));
        params.insert("comp-1".to_string(), comp_params);

        let explanation = explain_full_chain(&chain, &params);
        assert!(explanation.contains("2 effects"));
        assert!(explanation.contains("Parametric EQ"));
        assert!(explanation.contains("Compressor"));
        assert!(explanation.contains("threshold -18dB"));
        assert!(explanation.contains("ratio 4:1"));
    }

    #[test]
    fn test_explain_empty_chain() {
        let explanation = explain_full_chain(&[], &HashMap::new());
        assert!(explanation.contains("No effects"));
        assert!(explanation.contains("passing through clean"));
    }

    #[test]
    fn test_describe_eq_settings() {
        let mut params = HashMap::new();
        params.insert(
            "bands".to_string(),
            serde_json::json!([
                {"freq": 100, "gain": 3.0, "q": 1.0},
                {"freq": 3000, "gain": -2.0, "q": 1.5}
            ]),
        );

        let description = describe_eq_settings(&params);
        assert!(description.contains("+3dB @ 100Hz"));
        assert!(description.contains("-2dB @ 3000Hz"));
    }
}
