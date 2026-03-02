//! Meld CLI Binary
//!
//! Command-line interface for the Meld filesystem state management system.

use clap::Parser;
use meld::config::ConfigLoader;
use meld::logging::{init_logging, LoggingConfig};
use meld::cli::{Cli, RunContext};
use std::process;
use tracing::{error, info};

fn main() {
    let cli = Cli::parse();

    // Build logging config from CLI args, env vars, and config file
    let logging_config = build_logging_config(&cli);

    // Initialize logging early
    if let Err(e) = init_logging(Some(&logging_config)) {
        eprintln!("Failed to initialize logging: {}", e);
        process::exit(1);
    }

    info!("Meld CLI starting");

    // Create CLI context
    let context = match RunContext::new(cli.workspace.clone(), cli.config.clone()) {
        Ok(ctx) => {
            info!("CLI context initialized");
            ctx
        }
        Err(e) => {
            error!("Error initializing workspace: {}", e);
            eprintln!("{}", meld::cli::map_error(&e));
            process::exit(1);
        }
    };

    // Execute command
    match context.execute(&cli.command) {
        Ok(output) => {
            info!("Command completed successfully");
            println!("{}", output);
        }
        Err(e) => {
            error!("Command failed: {}", e);
            eprintln!("{}", meld::cli::map_error(&e));
            process::exit(1);
        }
    }
}

/// Build logging configuration from CLI args, environment, and config file.
/// Precedence: CLI flags override config file override defaults.
fn build_logging_config(cli: &Cli) -> LoggingConfig {
    let mut config = if let Some(ref config_path) = cli.config {
        ConfigLoader::load_from_file(config_path)
            .ok()
            .map(|c| c.logging)
            .unwrap_or_default()
    } else {
        ConfigLoader::load(&cli.workspace)
            .ok()
            .map(|c| c.logging)
            .unwrap_or_default()
    };

    if cli.quiet {
        config.enabled = false;
    }
    if cli.verbose {
        config.level = "debug".to_string();
        // Make verbose mode observable in terminal output without losing file logs.
        // An explicit --log-output value still takes precedence below.
        if config.output == "file" {
            config.output = "file+stderr".to_string();
        }
    }
    if let Some(ref level) = cli.log_level {
        config.level = level.clone();
    }
    if let Some(ref format) = cli.log_format {
        config.format = format.clone();
    }
    if let Some(ref output) = cli.log_output {
        config.output = output.clone();
    }

    let output_uses_file = config.output == "file" || config.output == "file+stderr";
    if config.enabled && output_uses_file {
        let resolved = meld::logging::resolve_log_file_path(
            cli.log_file.clone(),
            config.file.clone(),
            Some(cli.workspace.as_path()),
        );
        if let Ok(path) = resolved {
            config.file = Some(path);
        }
    } else if let Some(ref file) = cli.log_file {
        config.file = Some(file.clone());
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use meld::cli::Cli;

    #[test]
    fn test_build_logging_config_default() {
        let temp = tempfile::tempdir().unwrap();
        let ws = temp.path().to_string_lossy();
        let cli = Cli::try_parse_from(&["meld", "--workspace", ws.as_ref(), "status"]).unwrap();
        let config = build_logging_config(&cli);
        assert!(config.enabled, "default should have logging enabled");
        assert_eq!(config.output, "file", "default output should be file");
        assert_eq!(config.level, "info", "default level should be info");
    }

    #[test]
    fn test_build_logging_config_quiet() {
        let cli = Cli::try_parse_from(&["meld", "--quiet", "status"]).unwrap();
        let config = build_logging_config(&cli);
        assert!(!config.enabled, "quiet should disable logging");
    }

    #[test]
    fn test_build_logging_config_verbose() {
        let temp = tempfile::tempdir().unwrap();
        let ws = temp.path().to_string_lossy();
        let cli = Cli::try_parse_from(&[
            "meld",
            "--workspace",
            ws.as_ref(),
            "--verbose",
            "status",
        ])
        .unwrap();
        let config = build_logging_config(&cli);
        assert_eq!(config.level, "debug", "verbose should set level to debug");
        assert_eq!(
            config.output, "file+stderr",
            "verbose should mirror logs to stderr when default output is file"
        );
    }

    #[test]
    fn test_build_logging_config_verbose_respects_explicit_output_override() {
        let temp = tempfile::tempdir().unwrap();
        let ws = temp.path().to_string_lossy();
        let cli = Cli::try_parse_from(&[
            "meld",
            "--workspace",
            ws.as_ref(),
            "--verbose",
            "--log-output",
            "stderr",
            "status",
        ])
        .unwrap();
        let config = build_logging_config(&cli);
        assert_eq!(config.level, "debug");
        assert_eq!(
            config.output, "stderr",
            "explicit --log-output should win over verbose defaults"
        );
    }
}
