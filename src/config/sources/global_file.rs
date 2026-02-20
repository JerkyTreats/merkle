//! Global config file source: ~/.config/merkle/config.toml or $XDG_CONFIG_HOME/merkle/config.toml

use config::builder::DefaultState;
use config::ConfigBuilder;
use config::ConfigError;
use config::File;
use std::path::PathBuf;
use tracing::warn;

/// Path to global config file.
pub fn global_config_path() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("merkle")
            .join("config.toml")
    })
}

/// Add global config file source to builder if it exists.
/// Uses XDG_CONFIG_HOME when set, otherwise ~/.config/merkle/config.toml.
pub fn add_to_builder(
    mut builder: ConfigBuilder<DefaultState>,
) -> Result<ConfigBuilder<DefaultState>, ConfigError> {
    if let Some(xdg_config_path) = global_config_path() {
        if xdg_config_path.exists() {
            let canonical_xdg_path = xdg_config_path
                .canonicalize()
                .unwrap_or_else(|_| xdg_config_path.clone());
            builder = builder
                .add_source(File::with_name(canonical_xdg_path.to_str().unwrap()).required(false));
        } else {
            warn!(
                config_path = %xdg_config_path.display(),
                "Default configuration file not found at ~/.config/merkle/config.toml. \
                 Consider creating it for user-level defaults."
            );
        }
    }
    Ok(builder)
}
