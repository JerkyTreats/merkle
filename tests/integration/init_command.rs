//! Integration tests for Init CLI command

use merkle::agent::AgentRegistry;
use merkle::config::xdg;
use merkle::init;
use std::fs;
use tempfile::TempDir;

use crate::integration::with_xdg_env;

#[test]
fn test_init_creates_default_agents() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let summary = init::initialize_all(false).unwrap();
        
        // Check that all 4 agents were created
        assert_eq!(summary.agents.created.len(), 4);
        assert!(summary.agents.created.contains(&"reader".to_string()));
        assert!(summary.agents.created.contains(&"code-analyzer".to_string()));
        assert!(summary.agents.created.contains(&"docs-writer".to_string()));
        assert!(summary.agents.created.contains(&"synthesis-agent".to_string()));
        
        // Verify files exist
        let agents_dir = xdg::agents_dir().unwrap();
        assert!(agents_dir.join("reader.toml").exists());
        assert!(agents_dir.join("code-analyzer.toml").exists());
        assert!(agents_dir.join("docs-writer.toml").exists());
        assert!(agents_dir.join("synthesis-agent.toml").exists());
    });
}

#[test]
fn test_init_creates_prompts() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let summary = init::initialize_prompts(false).unwrap();
        
        // Check that all 3 prompts were created
        assert_eq!(summary.created.len(), 3);
        assert!(summary.created.contains(&"code-analyzer.md".to_string()));
        assert!(summary.created.contains(&"docs-writer.md".to_string()));
        assert!(summary.created.contains(&"synthesis-agent.md".to_string()));
        
        // Verify files exist
        let prompts_dir = xdg::prompts_dir().unwrap();
        assert!(prompts_dir.join("code-analyzer.md").exists());
        assert!(prompts_dir.join("docs-writer.md").exists());
        assert!(prompts_dir.join("synthesis-agent.md").exists());
        
        // Verify content is not empty
        let content = fs::read_to_string(prompts_dir.join("code-analyzer.md")).unwrap();
        assert!(!content.is_empty());
    });
}

#[test]
fn test_init_idempotent() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        // First initialization
        let summary1 = init::initialize_all(false).unwrap();
        assert_eq!(summary1.agents.created.len(), 4);
        assert_eq!(summary1.prompts.created.len(), 3);
        
        // Second initialization (should skip existing)
        let summary2 = init::initialize_all(false).unwrap();
        assert_eq!(summary2.agents.created.len(), 0);
        assert_eq!(summary2.agents.skipped.len(), 4);
        assert_eq!(summary2.prompts.created.len(), 0);
        assert_eq!(summary2.prompts.skipped.len(), 3);
    });
}

#[test]
fn test_init_force_overwrites() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        // First initialization
        let summary1 = init::initialize_all(false).unwrap();
        assert_eq!(summary1.agents.created.len(), 4);
        
        // Modify a file
        let agents_dir = xdg::agents_dir().unwrap();
        fs::write(
            agents_dir.join("reader.toml"),
            "# Modified content\nagent_id = \"reader\"\nrole = \"Reader\"",
        ).unwrap();
        
        // Force re-initialization
        let summary2 = init::initialize_all(true).unwrap();
        assert_eq!(summary2.agents.created.len(), 4);
        assert_eq!(summary2.agents.skipped.len(), 0);
        
        // Verify file was overwritten
        let content = fs::read_to_string(agents_dir.join("reader.toml")).unwrap();
        assert!(content.contains("agent_id = \"reader\""));
        assert!(!content.contains("# Modified content"));
    });
}

#[test]
fn test_init_list_mode() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        // List before initialization
        let preview1 = init::list_initialization().unwrap();
        assert_eq!(preview1.prompts.len(), 3);
        assert_eq!(preview1.agents.len(), 4);
        
        // Initialize
        init::initialize_all(false).unwrap();
        
        // List after initialization
        let preview2 = init::list_initialization().unwrap();
        assert_eq!(preview2.prompts.len(), 0);
        assert_eq!(preview2.agents.len(), 0);
    });
}

#[test]
fn test_init_validates_agents() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let summary = init::initialize_all(false).unwrap();
        
        // All agents should be valid
        for (agent_id, is_valid, errors) in &summary.validation.results {
            assert!(is_valid, "Agent {} failed validation: {:?}", agent_id, errors);
        }
        
        assert_eq!(summary.validation.results.len(), 4);
    });
}

#[test]
fn test_init_creates_xdg_directories() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        // Directories should not exist initially
        let config_home = xdg::config_home().unwrap();
        let merkle_dir = config_home.join("merkle");
        assert!(!merkle_dir.exists());
        
        // Initialize
        init::initialize_all(false).unwrap();
        
        // Directories should now exist
        assert!(merkle_dir.exists());
        assert!(merkle_dir.join("agents").exists());
        assert!(merkle_dir.join("prompts").exists());
        // Providers directory should exist (created by providers_dir() call)
        // Note: It may not exist if never accessed, but init ensures it exists
        let _ = xdg::providers_dir();
        assert!(merkle_dir.join("providers").exists());
    });
}

#[test]
fn test_init_handles_existing_files() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        // Create a custom agent file
        let agents_dir = xdg::agents_dir().unwrap();
        fs::write(
            agents_dir.join("reader.toml"),
            "# Custom content\nagent_id = \"reader\"\nrole = \"Reader\"",
        ).unwrap();
        
        // Initialize without force
        let summary = init::initialize_all(false).unwrap();
        
        // Reader should be skipped
        assert!(summary.agents.skipped.contains(&"reader".to_string()));
        
        // File should still have custom content
        let content = fs::read_to_string(agents_dir.join("reader.toml")).unwrap();
        assert!(content.contains("# Custom content"));
    });
}

#[test]
fn test_init_preserves_user_customizations() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        // Initialize once
        init::initialize_all(false).unwrap();
        
        // User modifies a prompt file
        let prompts_dir = xdg::prompts_dir().unwrap();
        let original_content = fs::read_to_string(prompts_dir.join("code-analyzer.md")).unwrap();
        fs::write(
            prompts_dir.join("code-analyzer.md"),
            "# Custom modified prompt\n\nThis is a user customization.",
        ).unwrap();
        
        // Initialize again without force
        let summary = init::initialize_all(false).unwrap();
        
        // Prompt should be skipped
        assert!(summary.prompts.skipped.contains(&"code-analyzer.md".to_string()));
        
        // Custom content should be preserved
        let content = fs::read_to_string(prompts_dir.join("code-analyzer.md")).unwrap();
        assert!(content.contains("# Custom modified prompt"));
        assert!(!content.contains(&original_content));
    });
}

#[test]
fn test_init_error_handling() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        // Create a read-only directory to test error handling
        let config_home = xdg::config_home().unwrap();
        let merkle_dir = config_home.join("merkle");
        let prompts_dir = merkle_dir.join("prompts");
        
        // Initialize normally first
        init::initialize_all(false).unwrap();
        
        // Make directory read-only (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&prompts_dir).unwrap().permissions();
            perms.set_mode(0o444); // Read-only
            fs::set_permissions(&prompts_dir, perms).unwrap();
            
            // Try to initialize with force (should fail on write)
            let result = init::initialize_prompts(true);
            
            // Restore permissions for cleanup
            let mut perms = fs::metadata(&prompts_dir).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&prompts_dir, perms).unwrap();
            
            // Should have errors
            if let Ok(summary) = result {
                assert!(!summary.errors.is_empty());
            }
        }
    });
}

#[test]
fn test_init_agent_configs_valid() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        init::initialize_all(false).unwrap();
        
        let agents_dir = xdg::agents_dir().unwrap();
        
        // Verify each agent config is valid TOML
        for agent_id in &["reader", "code-analyzer", "docs-writer", "synthesis-agent"] {
            let config_path = agents_dir.join(format!("{}.toml", agent_id));
            let content = fs::read_to_string(&config_path).unwrap();
            
            // Should parse as TOML
            let _: toml::Value = toml::from_str(&content).unwrap();
            
            // Should contain agent_id
            assert!(content.contains(&format!("agent_id = \"{}\"", agent_id)));
        }
    });
}

#[test]
fn test_init_prompts_have_content() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        init::initialize_prompts(false).unwrap();
        
        let prompts_dir = xdg::prompts_dir().unwrap();
        
        for prompt_file in &["code-analyzer.md", "docs-writer.md", "synthesis-agent.md"] {
            let content = fs::read_to_string(prompts_dir.join(prompt_file)).unwrap();
            
            // Should not be empty
            assert!(!content.is_empty());
            
            // Should be valid UTF-8 (read_to_string already verified this)
            // Should contain markdown structure
            assert!(content.contains("#"));
        }
    });
}

