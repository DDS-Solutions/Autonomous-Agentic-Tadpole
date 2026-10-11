//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Neural Billing**: Manages the centralized registry of AI model pricing.
//! Provides the **Financial Ground Truth** for USD cost calculation based
//! on token usage metrics. Enforces **SEC-02 (Synchronous Ledgers)** by
//! applying deterministic fallback rates for unrecognized models.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[rates]` in tracing logs.
//! - **Failure Path**: Model ID mismatch causing incorrect billing, or
//!   stale static rates for rapidly evolving provider pricing.
//! - **Trace Scope**: `server-rs::agent::rates`

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Represents the financial cost of a specific AI model.
/// Rates are defined as USD per 1,000 tokens for calculation granularity.
pub struct ModelRate {
    /// Cost in USD for 1,000 input (prompt) tokens.
    pub input_cost_per_1k: f64,
    /// Cost in USD for 1,000 output (completion) tokens.
    pub output_cost_per_1k: f64,
}

/// Static registry of model rates (Cost per 1,000 tokens)
pub static MODEL_RATES: Lazy<HashMap<&'static str, ModelRate>> = Lazy::new(|| {
    let mut m = HashMap::new();

    // OpenAI Models
    m.insert(
        "gpt-4o",
        ModelRate {
            input_cost_per_1k: 0.005,
            output_cost_per_1k: 0.015,
        },
    );
    m.insert(
        "gpt-4o-mini",
        ModelRate {
            input_cost_per_1k: 0.00015,
            output_cost_per_1k: 0.0006,
        },
    );

    // Anthropic Models
    m.insert(
        "claude-3-5-sonnet",
        ModelRate {
            input_cost_per_1k: 0.003,
            output_cost_per_1k: 0.015,
        },
    );
    m.insert(
        "claude-3-opus",
        ModelRate {
            input_cost_per_1k: 0.015,
            output_cost_per_1k: 0.075,
        },
    );

    m.insert(
        "gemini-3-pro-preview",
        ModelRate {
            input_cost_per_1k: 0.00125,
            output_cost_per_1k: 0.00375,
        },
    );
    m.insert(
        "gemini-3-flash-preview",
        ModelRate {
            input_cost_per_1k: 0.000075,
            output_cost_per_1k: 0.0003,
        },
    );
    // Legacy support
    m.insert(
        "gemini-1.5-pro",
        ModelRate {
            input_cost_per_1k: 0.00125,
            output_cost_per_1k: 0.00375,
        },
    );
    m.insert(
        "gemini-1.5-flash",
        ModelRate {
            input_cost_per_1k: 0.000075,
            output_cost_per_1k: 0.0003,
        },
    );

    // Groq (Llama 3.3 / Mixtral)
    m.insert(
        "llama-3.3-70b-versatile",
        ModelRate {
            input_cost_per_1k: 0.00059,
            output_cost_per_1k: 0.00079,
        },
    );
    m.insert(
        "mixtral-8x7b-32768",
        ModelRate {
            input_cost_per_1k: 0.00027,
            output_cost_per_1k: 0.00027,
        },
    );

    m
});

/// Calculates the cost in USD for a given token usage and model,
/// honoring custom per-million token rates when configured.
pub fn calculate_cost_with_custom_rates(
    custom_input_cost_per_m: Option<f64>,
    custom_output_cost_per_m: Option<f64>,
    model_id: &str,
    input_tokens: u32,
    output_tokens: u32,
) -> f64 {
    let lower = model_id.to_ascii_lowercase();
    if lower.starts_with("ollama")
        || lower.starts_with("local/")
        || lower.starts_with("mock")
        || lower == "null"
    {
        return 0.0;
    }

    if custom_input_cost_per_m.is_some() || custom_output_cost_per_m.is_some() {
        let in_cost_per_m = custom_input_cost_per_m.unwrap_or(0.0);
        let out_cost_per_m = custom_output_cost_per_m.unwrap_or(0.0);
        let input_cost = (input_tokens as f64 / 1_000_000.0) * in_cost_per_m;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * out_cost_per_m;
        tracing::debug!(
            "💰 [rates] Calculated custom cost for {}: in=${:.6} out=${:.6} (total=${:.6})",
            model_id,
            input_cost,
            output_cost,
            input_cost + output_cost
        );
        return input_cost + output_cost;
    }

    calculate_cost(model_id, input_tokens, output_tokens)
}

/// Calculates the cost in USD for a given token usage and model.
///
/// # Parameters
/// - `model_id`: The ID of the model used (e.g., "gpt-4o").
/// - `input_tokens`: The number of tokens sent in the request.
/// - `output_tokens`: The number of tokens received in the response.
///
/// # Returns
/// The calculated USD cost as an `f64`. If the model is not in the registry,
/// a standard fallback rate is applied.
pub fn calculate_cost(model_id: &str, input_tokens: u32, output_tokens: u32) -> f64 {
    let lower = model_id.to_ascii_lowercase();
    if lower.starts_with("ollama")
        || lower.starts_with("local/")
        || lower.starts_with("mock")
        || lower == "null"
    {
        return 0.0;
    }

    let rate = MODEL_RATES.get(model_id).unwrap_or(&ModelRate {
        input_cost_per_1k: 0.002, // Default fallback
        output_cost_per_1k: 0.006,
    });

    let input_cost = (input_tokens as f64 / 1000.0) * rate.input_cost_per_1k;
    let output_cost = (output_tokens as f64 / 1000.0) * rate.output_cost_per_1k;

    input_cost + output_cost
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_cost_gpt4o() {
        let cost = calculate_cost("gpt-4o", 1000, 1000);
        assert_eq!(cost, 0.005 + 0.015);
    }

    #[test]
    fn test_calculate_cost_unknown() {
        let cost = calculate_cost("unknown-model", 1000, 1000);
        // Default fallback: 0.002 + 0.006 = 0.008
        assert_eq!(cost, 0.008);
    }

    #[test]
    fn test_calculate_cost_gemini() {
        let cost = calculate_cost("gemini-1.5-flash", 10000, 10000);
        // input: 10 * 0.000075 = 0.00075
        // output: 10 * 0.0003 = 0.003
        assert!((cost - 0.00375).abs() < 1e-10);
    }

    #[test]
    fn test_calculate_cost_ollama() {
        let cost = calculate_cost("ollama/llama3:8b", 5000, 5000);
        assert_eq!(cost, 0.0);
        let cost_local = calculate_cost("local/qwen2.5", 5000, 5000);
        assert_eq!(cost_local, 0.0);
    }

    #[test]
    fn test_calculate_cost_with_custom_rates() {
        // $2.50 per 1M input, $10.00 per 1M output
        // 500,000 input tokens = 0.5 * 2.50 = 1.25
        // 200,000 output tokens = 0.2 * 10.00 = 2.00
        // total = 3.25
        let cost = calculate_cost_with_custom_rates(
            Some(2.5),
            Some(10.0),
            "custom-finetune",
            500_000,
            200_000,
        );
        assert!((cost - 3.25).abs() < 1e-10);
    }

    #[test]
    fn test_calculate_cost_with_custom_rates_local_zero() {
        let cost = calculate_cost_with_custom_rates(
            Some(5.0),
            Some(15.0),
            "ollama/llama3:70b",
            1_000_000,
            1_000_000,
        );
        assert_eq!(cost, 0.0);
    }
}

// Metadata: [rates]

