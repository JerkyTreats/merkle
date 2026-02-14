//! Integration tests for XDG configuration loading

use merkle::agent::{AgentRegistry, AgentRole};
use merkle::config::{
    resolve_prompt_path, xdg, MerkleConfig, PromptCache, ProviderConfig, ProviderType,
};
use merkle::provider::ProviderRegistry;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tempfile::TempDir;

use crate::integration::with_xdg_env;

// Mutex for tests that need direct environment variable manipulation
static XDG_CONFIG_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn test_xdg_config_home() {
    let _guard = XDG_CONFIG_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    // Test that config_home() respects XDG_CONFIG_HOME
    let original_xdg_config = std::env::var("XDG_CONFIG_HOME").ok();

    // Test with XDG_CONFIG_HOME set
    let test_dir = TempDir::new().unwrap();
    let test_config_home = test_dir.path().to_path_buf();
    std::env::set_var("XDG_CONFIG_HOME", test_config_home.to_str().unwrap());

    let config_home = xdg::config_home().unwrap();
    assert_eq!(config_home, test_config_home);

    // Test without XDG_CONFIG_HOME (should default to ~/.config)
    std::env::remove_var("XDG_CONFIG_HOME");
    let home = std::env::var("HOME").unwrap();
    let config_home = xdg::config_home().unwrap();
    assert_eq!(config_home, PathBuf::from(home).join(".config"));

    // Restore original
    if let Some(orig) = original_xdg_config {
        std::env::set_var("XDG_CONFIG_HOME", orig);
    } else {
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}

#[test]
fn test_xdg_agents_dir() {
    let _guard = XDG_CONFIG_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let test_dir = TempDir::new().unwrap();
    let test_config_home = test_dir.path().to_path_buf();
    let original_xdg_config = std::env::var("XDG_CONFIG_HOME").ok();
    std::env::set_var("XDG_CONFIG_HOME", test_config_home.to_str().unwrap());

    let agents_dir = xdg::agents_dir().unwrap();
    assert_eq!(agents_dir, test_config_home.join("merkle").join("agents"));
    assert!(agents_dir.exists());

    // Restore original
    if let Some(orig) = original_xdg_config {
        std::env::set_var("XDG_CONFIG_HOME", orig);
    } else {
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}

#[test]
fn test_xdg_providers_dir() {
    let _guard = XDG_CONFIG_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let test_dir = TempDir::new().unwrap();
    let test_config_home = test_dir.path().to_path_buf();
    let original_xdg_config = std::env::var("XDG_CONFIG_HOME").ok();
    std::env::set_var("XDG_CONFIG_HOME", test_config_home.to_str().unwrap());

    let providers_dir = xdg::providers_dir().unwrap();
    assert_eq!(
        providers_dir,
        test_config_home.join("merkle").join("providers")
    );
    assert!(providers_dir.exists());

    // Restore original
    if let Some(orig) = original_xdg_config {
        std::env::set_var("XDG_CONFIG_HOME", orig);
    } else {
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}

#[test]
fn test_resolve_prompt_path_absolute() {
    let base_dir = PathBuf::from("/tmp");
    let path = "/absolute/path/to/prompt.md";
    let resolved = resolve_prompt_path(path, &base_dir).unwrap();
    assert_eq!(resolved, PathBuf::from("/absolute/path/to/prompt.md"));
}

#[test]
fn test_resolve_prompt_path_tilde() {
    let home = std::env::var("HOME").unwrap();
    let base_dir = PathBuf::from("/tmp");
    let path = "~/prompts/test.md";
    let resolved = resolve_prompt_path(path, &base_dir).unwrap();
    assert_eq!(
        resolved,
        PathBuf::from(home).join("prompts").join("test.md")
    );
}

#[test]
fn test_resolve_prompt_path_relative_current_dir() {
    let current_dir = std::env::current_dir().unwrap();
    let base_dir = PathBuf::from("/tmp");
    let path = "./prompts/test.md";
    let resolved = resolve_prompt_path(path, &base_dir).unwrap();
    assert_eq!(resolved, current_dir.join("prompts").join("test.md"));
}

#[test]
fn test_resolve_prompt_path_relative_base() {
    let base_dir = PathBuf::from("/tmp/merkle");
    let path = "prompts/test.md";
    let resolved = resolve_prompt_path(path, &base_dir).unwrap();
    assert_eq!(resolved, base_dir.join("prompts").join("test.md"));
}

#[test]
fn test_prompt_cache() {
    let test_dir = TempDir::new().unwrap();
    let prompt_file = test_dir.path().join("prompt.md");
    fs::write(&prompt_file, "# Test Prompt\n\nThis is a test.").unwrap();

    let mut cache = PromptCache::new();

    // First load
    let content1 = cache.load_prompt(&prompt_file).unwrap();
    assert_eq!(content1, "# Test Prompt\n\nThis is a test.");

    // Second load should use cache
    let content2 = cache.load_prompt(&prompt_file).unwrap();
    assert_eq!(content2, content1);

    // Modify file
    fs::write(&prompt_file, "# Updated Prompt\n\nThis is updated.").unwrap();

    // Should reload after modification
    let content3 = cache.load_prompt(&prompt_file).unwrap();
    assert_eq!(content3, "# Updated Prompt\n\nThis is updated.");
}

#[test]
fn test_prompt_cache_empty_file() {
    let test_dir = TempDir::new().unwrap();
    let prompt_file = test_dir.path().join("empty.md");
    fs::write(&prompt_file, "   \n  ").unwrap(); // Only whitespace

    let mut cache = PromptCache::new();
    let result = cache.load_prompt(&prompt_file);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("empty"));
}

#[test]
fn test_provider_registry_load_from_xdg() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let providers_dir = xdg::providers_dir().unwrap();

        // Create a provider config
        let provider_file = providers_dir.join("test-ollama.toml");
        fs::write(
            &provider_file,
            r#"
provider_name = "test-ollama"
provider_type = "ollama"
model = "llama2"
endpoint = "http://localhost:11434"
"#,
        )
        .unwrap();

        let mut registry = ProviderRegistry::new();
        registry.load_from_xdg().unwrap();

        // Verify the provider was loaded
        let all_providers = registry.list_all();
        let provider_names: Vec<String> = all_providers
            .iter()
            .map(|p| {
                p.provider_name
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| "<no name>".to_string())
            })
            .collect();

        assert!(
            !all_providers.is_empty(),
            "At least one provider should be loaded. Found: {:?}",
            provider_names
        );

        let provider = registry.get("test-ollama").unwrap_or_else(|| {
            panic!(
                "Provider 'test-ollama' not found. Loaded providers: {:?}",
                provider_names
            )
        });
        assert_eq!(provider.model, "llama2");
        assert_eq!(provider.provider_type, ProviderType::Ollama);
    });
}

#[test]
fn test_provider_registry_load_from_xdg_invalid_skipped() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let providers_dir = xdg::providers_dir().unwrap();

        // Create a valid provider
        let valid_file = providers_dir.join("valid.toml");
        fs::write(
            &valid_file,
            r#"
provider_name = "valid"
provider_type = "ollama"
model = "llama2"
"#,
        )
        .unwrap();

        // Create an invalid provider (empty model)
        let invalid_file = providers_dir.join("invalid.toml");
        fs::write(
            &invalid_file,
            r#"
provider_name = "invalid"
provider_type = "ollama"
model = ""
"#,
        )
        .unwrap();

        let mut registry = ProviderRegistry::new();
        registry.load_from_xdg().unwrap();

        // Valid provider should be loaded
        assert!(registry.get("valid").is_some());

        // Invalid provider should be skipped
        assert!(registry.get("invalid").is_none());
    });
}

#[test]
fn test_agent_registry_load_from_xdg_inline_prompt() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let agents_dir = xdg::agents_dir().unwrap();

        // Create an agent config with inline prompt
        let agent_file = agents_dir.join("test-agent.toml");
        fs::write(
            &agent_file,
            r#"
agent_id = "test-agent"
role = "Writer"
system_prompt = "You are a test agent."
"#,
        )
        .unwrap();

        let mut registry = AgentRegistry::new();
        registry.load_from_xdg().unwrap();

        let agent = registry.get("test-agent").unwrap();
        assert_eq!(agent.role, AgentRole::Writer);
        assert_eq!(
            agent.metadata.get("system_prompt"),
            Some(&"You are a test agent.".to_string())
        );
    });
}

#[test]
fn test_agent_registry_load_from_xdg_prompt_path() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let agents_dir = xdg::agents_dir().unwrap();

        // Create a prompt file
        let prompt_file = test_dir.path().join("prompt.md");
        fs::write(&prompt_file, "# Test Agent\n\nYou are a helpful assistant.").unwrap();

        // Create an agent config with prompt path
        let agent_file = agents_dir.join("test-agent.toml");
        fs::write(
            &agent_file,
            &format!(
                r#"
agent_id = "test-agent"
role = "Writer"
system_prompt_path = "{}"
"#,
                prompt_file.display()
            ),
        )
        .unwrap();

        let mut registry = AgentRegistry::new();
        registry.load_from_xdg().unwrap();

        let agent = registry.get("test-agent").unwrap();
        assert_eq!(agent.role, AgentRole::Writer);
        assert_eq!(
            agent.metadata.get("system_prompt"),
            Some(&"# Test Agent\n\nYou are a helpful assistant.".to_string())
        );
    });
}

#[test]
fn test_agent_registry_load_from_xdg_prompt_path_relative() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let test_config_home = test_dir.path().to_path_buf();
        let agents_dir = xdg::agents_dir().unwrap();
        let merkle_dir = test_config_home.join("merkle");
        fs::create_dir_all(&merkle_dir.join("prompts")).unwrap();

        // Create a prompt file relative to XDG config
        let prompt_file = merkle_dir.join("prompts").join("test.md");
        fs::write(&prompt_file, "# Relative Prompt\n\nTest content.").unwrap();

        // Create an agent config with relative prompt path
        let agent_file = agents_dir.join("test-agent.toml");
        fs::write(
            &agent_file,
            r#"
agent_id = "test-agent"
role = "Writer"
system_prompt_path = "prompts/test.md"
"#,
        )
        .unwrap();

        let mut registry = AgentRegistry::new();
        registry.load_from_xdg().unwrap();

        let agent = registry.get("test-agent").unwrap();
        assert_eq!(
            agent.metadata.get("system_prompt"),
            Some(&"# Relative Prompt\n\nTest content.".to_string())
        );
    });
}

#[test]
fn test_agent_registry_load_from_xdg_prompt_path_tilde() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let home = std::env::var("HOME").unwrap();
        let home_prompts = PathBuf::from(&home).join("test_prompts");
        fs::create_dir_all(&home_prompts).unwrap();

        // Create a prompt file in home directory
        let prompt_file = home_prompts.join("test.md");
        fs::write(&prompt_file, "# Tilde Prompt\n\nFrom home directory.").unwrap();

        let agents_dir = xdg::agents_dir().unwrap();
        let agent_file = agents_dir.join("test-agent.toml");
        fs::write(
            &agent_file,
            &format!(
                r#"
agent_id = "test-agent"
role = "Writer"
system_prompt_path = "~/test_prompts/test.md"
"#,
            ),
        )
        .unwrap();

        let mut registry = AgentRegistry::new();
        registry.load_from_xdg().unwrap();

        let agent = registry.get("test-agent").unwrap();
        assert_eq!(
            agent.metadata.get("system_prompt"),
            Some(&"# Tilde Prompt\n\nFrom home directory.".to_string())
        );

        // Cleanup
        fs::remove_file(&prompt_file).ok();
        fs::remove_dir(&home_prompts).ok();
    });
}

#[test]
fn test_agent_registry_load_from_xdg_missing_prompt_skipped() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let agents_dir = xdg::agents_dir().unwrap();

        // Create an agent config with non-existent prompt path
        let agent_file = agents_dir.join("test-agent.toml");
        fs::write(
            &agent_file,
            r#"
agent_id = "test-agent"
role = "Writer"
system_prompt_path = "/nonexistent/prompt.md"
"#,
        )
        .unwrap();

        let mut registry = AgentRegistry::new();
        registry.load_from_xdg().unwrap();

        // Agent should be skipped due to missing prompt file
        assert!(registry.get("test-agent").is_none());
    });
}

#[test]
fn test_agent_registry_load_from_xdg_reader_no_prompt() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let agents_dir = xdg::agents_dir().unwrap();

        // Create a Reader agent without prompt (should be valid)
        let agent_file = agents_dir.join("reader.toml");
        fs::write(
            &agent_file,
            r#"
agent_id = "reader"
role = "Reader"
"#,
        )
        .unwrap();

        let mut registry = AgentRegistry::new();
        registry.load_from_xdg().unwrap();

        let agent = registry.get("reader").unwrap();
        assert_eq!(agent.role, AgentRole::Reader);
        // Reader agents don't need prompts
        assert!(agent.metadata.get("system_prompt").is_none());
    });
}

#[test]
fn test_agent_registry_load_from_xdg_writer_missing_prompt_skipped() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let agents_dir = xdg::agents_dir().unwrap();

        // Create a Writer agent without prompt (should be skipped)
        let agent_file = agents_dir.join("writer.toml");
        fs::write(
            &agent_file,
            r#"
agent_id = "writer"
role = "Writer"
"#,
        )
        .unwrap();

        let mut registry = AgentRegistry::new();
        registry.load_from_xdg().unwrap();

        // Writer agent without prompt should be skipped
        assert!(registry.get("writer").is_none());
    });
}

#[test]
fn test_load_order_xdg_overrides_config() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        // Create a config.toml with a provider
        let mut config = MerkleConfig::default();
        let provider_config = ProviderConfig {
            provider_name: Some("test-provider".to_string()),
            provider_type: ProviderType::Ollama,
            model: "llama2".to_string(),
            api_key: None,
            endpoint: None,
            default_options: Default::default(),
        };
        config
            .providers
            .insert("test-provider".to_string(), provider_config.clone());

        // Create XDG provider with same name but different model
        let providers_dir = xdg::providers_dir().unwrap();
        let provider_file = providers_dir.join("test-provider.toml");
        fs::write(
            &provider_file,
            r#"
provider_name = "test-provider"
provider_type = "ollama"
model = "llama3"
"#,
        )
        .unwrap();

        let mut registry = ProviderRegistry::new();
        registry.load_from_config(&config).unwrap();
        registry.load_from_xdg().unwrap();

        // XDG provider should override config.toml provider
        let provider = registry.get("test-provider").unwrap();
        assert_eq!(provider.model, "llama3"); // XDG version
    });
}

#[test]
fn test_agent_id_filename_mismatch_warning() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let agents_dir = xdg::agents_dir().unwrap();

        // Create an agent config where agent_id doesn't match filename
        let agent_file = agents_dir.join("filename.toml");
        fs::write(
            &agent_file,
            r#"
agent_id = "different-id"
role = "Writer"
system_prompt = "Test prompt"
"#,
        )
        .unwrap();

        let mut registry = AgentRegistry::new();
        registry.load_from_xdg().unwrap();

        // Agent should still be loaded (with warning logged)
        let agent = registry.get("different-id").unwrap();
        assert_eq!(agent.role, AgentRole::Writer);
    });
}
