//! Logging System
//!
//! Structured logging implementation using the `tracing` crate. Provides configurable
//! log levels, output formats, and destinations as specified in the logging specification.

use crate::error::ApiError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing_subscriber::fmt::time::ChronoUtc;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level: trace, debug, info, warn, error, off
    #[serde(default = "default_log_level")]
    pub level: String,

    /// Output format: json, text (default: text)
    #[serde(default = "default_format")]
    pub format: String,

    /// Output destination: stdout, stderr, file, both
    #[serde(default = "default_output")]
    pub output: String,

    /// Log file path (if output includes "file")
    #[serde(default = "default_log_file")]
    pub file: PathBuf,

    /// Enable log rotation
    #[serde(default = "default_true")]
    pub rotation: bool,

    /// Maximum log file size before rotation (bytes)
    #[serde(default = "default_max_file_size")]
    pub max_file_size: u64,

    /// Number of rotated log files to keep
    #[serde(default = "default_max_files")]
    pub max_files: usize,

    /// Enable colored output (text format only, stdout/stderr only)
    #[serde(default = "default_true")]
    pub color: bool,

    /// Module-specific log levels
    #[serde(default)]
    pub modules: HashMap<String, String>,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_format() -> String {
    "text".to_string()
}

fn default_output() -> String {
    "stdout".to_string()
}

fn default_log_file() -> PathBuf {
    // This is a placeholder - actual path is computed at runtime
    // The path will be resolved to $XDG_DATA_HOME/merkle/workspaces/<hash>/merkle.log
    // when logging is initialized with a workspace root
    PathBuf::from(".merkle/merkle.log")
}

fn default_true() -> bool {
    true
}

fn default_max_file_size() -> u64 {
    10 * 1024 * 1024 // 10 MB
}

fn default_max_files() -> usize {
    5
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_format(),
            output: default_output(),
            file: default_log_file(),
            rotation: default_true(),
            max_file_size: default_max_file_size(),
            max_files: default_max_files(),
            color: default_true(),
            modules: HashMap::new(),
        }
    }
}

/// Initialize the logging system
///
/// Priority order (highest to lowest):
/// 1. CLI arguments (passed via env vars or direct config)
/// 2. Environment variables (MERKLE_LOG, MERKLE_LOG_FORMAT, etc.)
/// 3. Configuration file
/// 4. Defaults
pub fn init_logging(config: Option<&LoggingConfig>) -> Result<(), ApiError> {
    // Build filter from environment or config
    let filter = build_env_filter(config)?;

    // Determine format
    let format = determine_format(config)?;

    // Determine output destinations
    let output = determine_output(config)?;

    // Build subscriber - start with registry and filter
    let base_subscriber = Registry::default().with(filter);

    // Determine if we should use color
    let use_color = config.map(|c| c.color).unwrap_or(true);

    // Helper to get or create log file writer
    let get_file_writer = || -> Result<std::fs::File, ApiError> {
        let log_file = config
            .map(|c| c.file.clone())
            .unwrap_or_else(default_log_file);

        if let Some(parent) = log_file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ApiError::ConfigError(format!("Failed to create log directory: {}", e))
            })?;
        }
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
            .map_err(|e| {
                ApiError::ConfigError(format!("Failed to open log file {:?}: {}", log_file, e))
            })
    };

    // Build subscriber based on format
    // Support stdout (default) or file output
    // Note: stderr and multiple outputs require more complex type handling and can be added later
    if format == "json" {
        // JSON format
        if output.file {
            let file_writer = get_file_writer()?;
            base_subscriber
                .with(
                    fmt::layer()
                        .json()
                        .with_target(true)
                        .with_timer(ChronoUtc::rfc_3339())
                        .with_writer(file_writer),
                )
                .init();
        } else {
            // Default to stdout
            base_subscriber
                .with(
                    fmt::layer()
                        .json()
                        .with_target(true)
                        .with_timer(ChronoUtc::rfc_3339())
                        .with_writer(std::io::stdout),
                )
                .init();
        }
    } else {
        // Text format
        if output.file {
            let file_writer = get_file_writer()?;
            base_subscriber
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_timer(ChronoUtc::rfc_3339())
                        .with_ansi(false)
                        .with_writer(file_writer),
                )
                .init();
        } else {
            // Default to stdout
            base_subscriber
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_timer(ChronoUtc::rfc_3339())
                        .with_ansi(use_color)
                        .with_writer(std::io::stdout),
                )
                .init();
        }
    }

    Ok(())
}

/// Build environment filter from config or environment variables
fn build_env_filter(config: Option<&LoggingConfig>) -> Result<EnvFilter, ApiError> {
    // First, try to get filter from MERKLE_LOG environment variable
    let env_filter = EnvFilter::try_from_env("MERKLE_LOG");

    if let Ok(filter) = env_filter {
        return Ok(filter);
    }

    // Build filter from config
    let level = config.map(|c| c.level.as_str()).unwrap_or("info");

    if level == "off" {
        return Ok(EnvFilter::new("off"));
    }

    let mut filter = EnvFilter::new(level);

    // Add module-specific filters
    if let Some(config) = config {
        for (module, module_level) in &config.modules {
            let directive = format!("{}={}", module, module_level);
            filter = filter.add_directive(
                directive
                    .parse()
                    .map_err(|e| ApiError::ConfigError(format!("Invalid log directive: {}", e)))?,
            );
        }
    }

    // Also check MERKLE_LOG_MODULES environment variable
    if let Ok(modules_str) = std::env::var("MERKLE_LOG_MODULES") {
        for module_spec in modules_str.split(',') {
            let parts: Vec<&str> = module_spec.split('=').collect();
            if parts.len() == 2 {
                let directive = format!("{}={}", parts[0].trim(), parts[1].trim());
                filter = filter.add_directive(directive.parse().map_err(|e| {
                    ApiError::ConfigError(format!("Invalid log directive from env: {}", e))
                })?);
            }
        }
    }

    Ok(filter)
}

/// Determine output format from config or environment
fn determine_format(config: Option<&LoggingConfig>) -> Result<String, ApiError> {
    // Check environment variable first
    if let Ok(format) = std::env::var("MERKLE_LOG_FORMAT") {
        if format == "json" || format == "text" {
            return Ok(format);
        }
    }

    // Use config
    let format = config.map(|c| c.format.as_str()).unwrap_or("text");

    if format != "json" && format != "text" {
        return Err(ApiError::ConfigError(format!(
            "Invalid log format: {} (must be 'json' or 'text')",
            format
        )));
    }

    Ok(format.to_string())
}

/// Output destinations
struct OutputDestinations {
    #[allow(dead_code)] // Planned for future use (see comment in init_logging)
    stdout: bool,
    #[allow(dead_code)] // Planned for future use (see comment in init_logging)
    stderr: bool,
    file: bool,
}

/// Determine output destinations from config or environment
fn determine_output(config: Option<&LoggingConfig>) -> Result<OutputDestinations, ApiError> {
    // Check environment variable first
    if let Ok(output) = std::env::var("MERKLE_LOG_OUTPUT") {
        return parse_output_destinations(&output);
    }

    // Use config
    let output = config.map(|c| c.output.as_str()).unwrap_or("stdout");

    parse_output_destinations(output)
}

fn parse_output_destinations(output: &str) -> Result<OutputDestinations, ApiError> {
    match output {
        "stdout" => Ok(OutputDestinations {
            stdout: true,
            stderr: false,
            file: false,
        }),
        "stderr" => Ok(OutputDestinations {
            stdout: false,
            stderr: true,
            file: false,
        }),
        "file" => Ok(OutputDestinations {
            stdout: false,
            stderr: false,
            file: true,
        }),
        "both" => Ok(OutputDestinations {
            stdout: true,
            stderr: true,
            file: false,
        }),
        _ => Err(ApiError::ConfigError(format!(
            "Invalid log output: {} (must be 'stdout', 'stderr', 'file', or 'both')",
            output
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_logging_config() {
        let config = LoggingConfig::default();
        assert_eq!(config.level, "info");
        assert_eq!(config.format, "text");
        assert_eq!(config.output, "stdout");
        assert!(config.color);
    }

    #[test]
    fn test_parse_output_destinations() {
        let out = parse_output_destinations("stdout").unwrap();
        assert!(out.stdout);
        assert!(!out.stderr);
        assert!(!out.file);

        let out = parse_output_destinations("both").unwrap();
        assert!(out.stdout);
        assert!(out.stderr);
        assert!(!out.file);
    }
}
