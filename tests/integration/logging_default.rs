//! Integration tests for default log-to-file behavior.
//!
//! Verifies that running the CLI without --quiet writes logs to the default
//! file path under the platform state directory.

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Matches default_log_file_path in src/logging.rs: state_dir is
/// $XDG_STATE_HOME/meld (ProjectDirs app path), then workspace segments are appended.
fn expected_log_path(state_home: &Path, workspace: &Path) -> std::path::PathBuf {
    let canonical = workspace.canonicalize().unwrap();
    let mut base = state_home.join("meld");
    for component in canonical.components() {
        match component {
            std::path::Component::RootDir
            | std::path::Component::Prefix(_)
            | std::path::Component::CurDir
            | std::path::Component::ParentDir => {}
            std::path::Component::Normal(name) => {
                base = base.join(name);
            }
        }
    }
    base.join("meld.log")
}

#[test]
fn test_default_logging_writes_to_file() {
    let temp_dir = TempDir::new().unwrap();
    let state_home = temp_dir.path().to_path_buf();
    let data_home = temp_dir.path().join("data");
    let config_home = temp_dir.path().join("config");
    let home = temp_dir.path().join("home");
    let workspace = temp_dir.path().join("ws");
    fs::create_dir_all(&data_home).unwrap();
    fs::create_dir_all(&config_home).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();

    let bin = env!("CARGO_BIN_EXE_meld");
    let output = Command::new(bin)
        .env("XDG_STATE_HOME", state_home.as_os_str())
        .env("XDG_DATA_HOME", data_home.as_os_str())
        .env("XDG_CONFIG_HOME", config_home.as_os_str())
        .env("HOME", home.as_os_str())
        .arg("--workspace")
        .arg(&workspace)
        .arg("status")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "meld status should succeed: stderr={:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    let log_path = expected_log_path(&state_home, &workspace);
    assert!(
        log_path.exists(),
        "log file should exist at {}",
        log_path.display()
    );
    let content = fs::read_to_string(&log_path).unwrap();
    assert!(
        content.contains("Meld CLI starting") || content.contains("info"),
        "log file should contain a startup message; got: {}",
        content.lines().next().unwrap_or("")
    );
}

#[test]
fn test_verbose_logging_mirrors_to_stderr_and_file() {
    let temp_dir = TempDir::new().unwrap();
    let state_home = temp_dir.path().to_path_buf();
    let data_home = temp_dir.path().join("data");
    let config_home = temp_dir.path().join("config");
    let home = temp_dir.path().join("home");
    let workspace = temp_dir.path().join("ws");
    fs::create_dir_all(&data_home).unwrap();
    fs::create_dir_all(&config_home).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();

    let bin = env!("CARGO_BIN_EXE_meld");
    let output = Command::new(bin)
        .env("XDG_STATE_HOME", state_home.as_os_str())
        .env("XDG_DATA_HOME", data_home.as_os_str())
        .env("XDG_CONFIG_HOME", config_home.as_os_str())
        .env("HOME", home.as_os_str())
        .arg("--workspace")
        .arg(&workspace)
        .arg("--verbose")
        .arg("status")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "meld --verbose status should succeed: stderr={:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.trim().is_empty(),
        "verbose mode should emit logs to stderr"
    );

    let log_path = expected_log_path(&state_home, &workspace);
    assert!(
        log_path.exists(),
        "log file should exist at {}",
        log_path.display()
    );
}
