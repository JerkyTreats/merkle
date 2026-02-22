//! Provider command presentation: list, show, validation, test text/json.

use crate::provider::commands::{
    ProviderListResult, ProviderShowResult, ProviderTestResult,
};
use crate::provider::profile::provider_type_slug;
use crate::provider::ValidationResult;
use serde_json::json;

pub fn format_provider_list_result_text(result: &ProviderListResult) -> String {
    let providers = &result.providers;
    if providers.is_empty() {
        return "No providers found.\n\nUse 'merkle provider create' to add a provider.".to_string();
    }
    let mut output = String::from("Available Providers:\n");
    for provider in providers {
        let type_str = provider_type_slug(provider.provider_type);
        let endpoint_str = provider.endpoint.as_deref().unwrap_or("(default endpoint)");
        let provider_name = provider.provider_name.as_deref().unwrap_or("unknown");
        output.push_str(&format!(
            "  {:<20} {:<10} {:<20} {}\n",
            provider_name, type_str, provider.model, endpoint_str
        ));
    }
    output.push_str(&format!("\nTotal: {} provider(s)\n", providers.len()));
    output
}

pub fn format_provider_list_result_json(result: &ProviderListResult) -> String {
    let provider_list: Vec<_> = result
        .providers
        .iter()
        .map(|provider| {
            let type_str = provider_type_slug(provider.provider_type);
            json!({
                "provider_name": provider.provider_name.as_deref().unwrap_or("unknown"),
                "provider_type": type_str,
                "model": provider.model,
                "endpoint": provider.endpoint,
            })
        })
        .collect();
    let out = json!({ "providers": provider_list, "total": result.providers.len() });
    serde_json::to_string_pretty(&out).unwrap_or_else(|_| "{}".to_string())
}

pub fn format_provider_show_result_text(result: &ProviderShowResult) -> String {
    let provider = &result.config;
    let mut output = format!(
        "Provider: {}\n",
        provider.provider_name.as_deref().unwrap_or("unknown")
    );
    let type_str = provider_type_slug(provider.provider_type);
    output.push_str(&format!("Type: {}\n", type_str));
    output.push_str(&format!("Model: {}\n", provider.model));
    if let Some(endpoint) = &provider.endpoint {
        output.push_str(&format!("Endpoint: {}\n", endpoint));
    } else {
        output.push_str("Endpoint: (default endpoint)\n");
    }
    if let Some(status) = &result.api_key_status {
        output.push_str(&format!("API Key: {}\n", status));
    }
    output.push_str("\nDefault Completion Options:\n");
    if let Some(temp) = provider.default_options.temperature {
        output.push_str(&format!("  temperature: {}\n", temp));
    }
    if let Some(max_tokens) = provider.default_options.max_tokens {
        output.push_str(&format!("  max_tokens: {}\n", max_tokens));
    }
    if let Some(top_p) = provider.default_options.top_p {
        output.push_str(&format!("  top_p: {}\n", top_p));
    }
    if let Some(freq_penalty) = provider.default_options.frequency_penalty {
        output.push_str(&format!("  frequency_penalty: {}\n", freq_penalty));
    }
    if let Some(pres_penalty) = provider.default_options.presence_penalty {
        output.push_str(&format!("  presence_penalty: {}\n", pres_penalty));
    }
    if let Some(ref stop) = provider.default_options.stop {
        output.push_str(&format!("  stop: {:?}\n", stop));
    }
    output
}

pub fn format_provider_show_result_json(result: &ProviderShowResult) -> String {
    let provider = &result.config;
    let type_str = provider_type_slug(provider.provider_type);
    let api_key_status_str = result.api_key_status.as_deref().map(|s| match s {
        s if s.contains("from config") => "set_from_config",
        s if s.contains("from environment") => "set_from_env",
        s if s.contains("Not set") => "not_set",
        s if s.contains("Not required") => "not_required",
        _ => "unknown",
    });
    let default_options = json!({
        "temperature": provider.default_options.temperature,
        "max_tokens": provider.default_options.max_tokens,
        "top_p": provider.default_options.top_p,
        "frequency_penalty": provider.default_options.frequency_penalty,
        "presence_penalty": provider.default_options.presence_penalty,
        "stop": provider.default_options.stop,
    });
    let out = json!({
        "provider_name": provider.provider_name.as_deref().unwrap_or("unknown"),
        "provider_type": type_str,
        "model": provider.model,
        "endpoint": provider.endpoint,
        "api_key_status": api_key_status_str,
        "default_options": default_options,
    });
    serde_json::to_string_pretty(&out).unwrap_or_else(|_| "{}".to_string())
}

pub fn format_provider_validation_result(
    result: &ValidationResult,
    verbose: bool,
) -> String {
    let mut output = format!("Validating provider: {}\n\n", result.provider_name);

    if result.errors.is_empty() && result.checks.iter().all(|(_, passed)| *passed) {
        output.push_str("✓ All validation checks passed\n\n");
    } else {
        for (description, passed) in &result.checks {
            if *passed {
                output.push_str(&format!("✓ {}\n", description));
            } else {
                output.push_str(&format!("✗ {}\n", description));
            }
        }
        if !result.errors.is_empty() {
            output.push_str("\nErrors:\n");
            for error in &result.errors {
                output.push_str(&format!("✗ {}\n", error));
            }
        }
        if !result.warnings.is_empty() {
            output.push_str("\nWarnings:\n");
            for warning in &result.warnings {
                output.push_str(&format!("⚠ {}\n", warning));
            }
        }
        output.push_str(&format!(
            "\nValidation {}: {}/{} checks passed, {} errors found\n",
            if result.is_valid() {
                "passed"
            } else {
                "failed"
            },
            result.passed_checks(),
            result.total_checks(),
            result.errors.len()
        ));
    }

    if verbose {
        output.push_str(&format!("\nTotal checks: {}\n", result.total_checks()));
        output.push_str(&format!("Passed: {}\n", result.passed_checks()));
        output.push_str(&format!("Errors: {}\n", result.errors.len()));
        output.push_str(&format!("Warnings: {}\n", result.warnings.len()));
    }
    output
}

pub fn format_provider_test_result(
    result: &ProviderTestResult,
    elapsed_ms: Option<u128>,
) -> String {
    let mut output = format!("Testing provider: {}\n\n", result.provider_name);
    output.push_str("✓ Provider client created\n");
    if result.connectivity_ok {
        output.push_str(&match elapsed_ms {
            Some(ms) => format!("✓ API connectivity: OK ({}ms)\n", ms),
            None => "✓ API connectivity: OK\n".to_string(),
        });
        if result.model_available {
            output.push_str(&format!("✓ Model '{}' is available\n", result.model_checked));
        } else {
            output.push_str(&format!("✗ Model '{}' not found\n", result.model_checked));
            output.push_str(&format!(
                "Available models: {}\n",
                result.available_models.join(", ")
            ));
            return output;
        }
    } else {
        if let Some(ref msg) = result.error_message {
            output.push_str(&format!("✗ API connectivity failed: {}\n", msg));
        }
        return output;
    }
    output.push_str("\nProvider is working correctly.\n");
    output
}
