//! Merge rules: defaults, override order, conflict handling.

use config::Config;
use config::ConfigBuilder;
use config::ConfigError;

/// Create a Config builder with merge policy defaults applied.
pub fn builder_with_defaults() -> Result<ConfigBuilder<config::builder::DefaultState>, ConfigError>
{
    Config::builder()
        .set_default("system.default_workspace_root", ".")?
        .set_default("system.storage.store_path", ".merkle/store")?
        .set_default("system.storage.frames_path", ".merkle/frames")
}
