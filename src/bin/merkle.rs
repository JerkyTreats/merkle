//! Merkle CLI Binary
//!
//! Command-line interface for the Merkle filesystem state management system.

use clap::Parser;
use merkle::config::ConfigLoader;
use merkle::logging::{init_logging, LoggingConfig};
use merkle::cli::{Cli, RunContext};
use std::path::PathBuf;
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

    info!("Merkle CLI starting");

    // Create CLI context
    let context = match RunContext::new(cli.workspace.clone(), cli.config.clone()) {
        Ok(ctx) => {
            info!("CLI context initialized");
            ctx
        }
        Err(e) => {
            error!("Error initializing workspace: {}", e);
            eprintln!("{}", merkle::cli::map_error(&e));
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
            eprintln!("{}", merkle::cli::map_error(&e));
            process::exit(1);
        }
    }
}

/// Build logging configuration from CLI args, environment, and config file
fn build_logging_config(cli: &Cli) -> LoggingConfig {
    // If --verbose is not set, disable logging
    if !cli.verbose {
        let mut config = LoggingConfig::default();
        config.level = "off".to_string();
        return config;
    }

    // Try to load config file first
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

    // Override with CLI arguments (highest priority)
    if let Some(ref level) = cli.log_level {
        config.level = level.clone();
    }
    if let Some(ref format) = cli.log_format {
        config.format = format.clone();
    }
    if let Some(ref output) = cli.log_output {
        config.output = output.clone();
    }
    if let Some(ref file) = cli.log_file {
        config.file = file.clone();
    } else if config.file == PathBuf::from(".merkle/merkle.log") {
        // Resolve default log file path to XDG data directory
        if let Ok(data_dir) = merkle::config::xdg::workspace_data_dir(&cli.workspace) {
            config.file = data_dir.join("merkle.log");
        }
    }

    config
}
