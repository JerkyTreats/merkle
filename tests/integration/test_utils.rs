//! Shared test utilities for integration tests
//!
//! Provides centralized setup/teardown for XDG directories and other test resources
//! to avoid code duplication and ensure consistent test isolation.

use std::sync::Mutex;
use tempfile::TempDir;

/// Global mutex to serialize XDG environment variable access across all tests
/// This prevents race conditions when tests run in parallel
static XDG_ENV_MUTEX: Mutex<()> = Mutex::new(());

/// Environment variable state to restore after test
struct EnvState {
    home: Option<String>,
    xdg_config_home: Option<String>,
    xdg_data_home: Option<String>,
}

impl EnvState {
    fn capture() -> Self {
        Self {
            home: std::env::var("HOME").ok(),
            xdg_config_home: std::env::var("XDG_CONFIG_HOME").ok(),
            xdg_data_home: std::env::var("XDG_DATA_HOME").ok(),
        }
    }

    fn restore(self) {
        if let Some(orig) = self.home {
            std::env::set_var("HOME", orig);
        } else {
            std::env::remove_var("HOME");
        }

        if let Some(orig) = self.xdg_config_home {
            std::env::set_var("XDG_CONFIG_HOME", orig);
        } else {
            std::env::remove_var("XDG_CONFIG_HOME");
        }

        if let Some(orig) = self.xdg_data_home {
            std::env::set_var("XDG_DATA_HOME", orig);
        } else {
            std::env::remove_var("XDG_DATA_HOME");
        }
    }
}

/// Set up isolated XDG directories for a test with automatic cleanup
///
/// This function:
/// - Creates isolated XDG_CONFIG_HOME and XDG_DATA_HOME directories in the temp dir
/// - Sets HOME to ensure fallback paths work correctly
/// - Automatically restores original environment variables after the test
/// - Uses a global mutex to prevent race conditions in parallel test execution
///
/// # Example
/// ```
/// use tempfile::TempDir;
/// use crate::test_utils::with_xdg_env;
///
/// let test_dir = TempDir::new().unwrap();
/// with_xdg_env(&test_dir, || {
///     // Your test code here
///     // XDG_CONFIG_HOME and XDG_DATA_HOME are set to test_dir
/// });
/// // Environment automatically restored
/// ```
pub fn with_xdg_env<F, R>(test_dir: &TempDir, f: F) -> R
where
    F: FnOnce() -> R,
{
    let _guard = XDG_ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let env_state = EnvState::capture();

    // Set up test directories
    let test_config_home = test_dir.path().to_path_buf();
    let test_data_home = test_dir.path().join("data");
    let test_home = test_dir.path().join("home");

    std::fs::create_dir_all(&test_data_home).unwrap();
    std::fs::create_dir_all(&test_home).unwrap();

    // Set environment variables
    std::env::set_var("HOME", test_home.to_str().unwrap());
    std::env::set_var("XDG_CONFIG_HOME", test_config_home.to_str().unwrap());
    std::env::set_var("XDG_DATA_HOME", test_data_home.to_str().unwrap());

    // Run test
    let result = f();

    // Restore original environment
    env_state.restore();

    result
}

/// Set up only XDG_DATA_HOME (for workspace isolation tests)
///
/// This is a lighter-weight version for tests that only need data directory isolation
/// but don't need config directory isolation.
pub fn with_xdg_data_home<F, R>(test_dir: &TempDir, f: F) -> R
where
    F: FnOnce() -> R,
{
    let _guard = XDG_ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let env_state = EnvState::capture();

    let test_data_home = test_dir.path().join("data");
    let test_home = test_dir.path().join("home");

    std::fs::create_dir_all(&test_data_home).unwrap();
    std::fs::create_dir_all(&test_home).unwrap();

    std::env::set_var("HOME", test_home.to_str().unwrap());
    std::env::set_var("XDG_DATA_HOME", test_data_home.to_str().unwrap());

    let result = f();

    env_state.restore();

    result
}
