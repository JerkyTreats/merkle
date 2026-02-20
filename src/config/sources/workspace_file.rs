//! Workspace config file source: config/config.toml and config/{env}.toml

use config::builder::DefaultState;
use config::ConfigBuilder;
use config::ConfigError;
use config::File;
use std::path::Path;

/// Add workspace config files to builder.
/// Precedence: config/config.toml (base) then config/{MERKLE_ENV}.toml (env-specific).
pub fn add_to_builder(
    builder: ConfigBuilder<DefaultState>,
    workspace_root: &Path,
) -> Result<ConfigBuilder<DefaultState>, ConfigError> {
    let config_dir = workspace_root.join("config");
    let env_name = std::env::var("MERKLE_ENV").unwrap_or_else(|_| "development".to_string());

    let mut builder = builder;

    let base_config_path = config_dir.join("config.toml");
    if base_config_path.exists() {
        builder =
            builder.add_source(File::with_name(base_config_path.to_str().unwrap()).required(false));
    }

    let env_config_path = config_dir.join(format!("{}.toml", env_name));
    if env_config_path.exists() {
        builder =
            builder.add_source(File::with_name(env_config_path.to_str().unwrap()).required(false));
    }

    Ok(builder)
}
