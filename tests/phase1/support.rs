use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use merkle::agent::{AgentIdentity, AgentRegistry, AgentRole, AgentStorage, XdgAgentStorage};
use merkle::api::ContextApi;
use merkle::concurrency::NodeLockManager;
use merkle::config::{xdg, AgentConfig, ProviderConfig, ProviderType};
use merkle::context::frame::storage::FrameStorage;
use merkle::heads::HeadIndex;
use merkle::provider::ProviderRegistry;
use merkle::store::persistence::SledNodeRecordStore;
use merkle::telemetry::ProgressRuntime;
use tempfile::TempDir;

static XDG_ENV_MUTEX: Mutex<()> = Mutex::new(());

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
        if let Some(home) = self.home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }

        if let Some(xdg_config_home) = self.xdg_config_home {
            std::env::set_var("XDG_CONFIG_HOME", xdg_config_home);
        } else {
            std::env::remove_var("XDG_CONFIG_HOME");
        }

        if let Some(xdg_data_home) = self.xdg_data_home {
            std::env::set_var("XDG_DATA_HOME", xdg_data_home);
        } else {
            std::env::remove_var("XDG_DATA_HOME");
        }
    }
}

pub fn with_xdg_env<F, R>(temp_dir: &TempDir, f: F) -> R
where
    F: FnOnce() -> R,
{
    let _guard = XDG_ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let env_state = EnvState::capture();

    let home = temp_dir.path().join("home");
    let config_home = temp_dir.path().to_path_buf();
    let data_home = temp_dir.path().join("data");

    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config_home).unwrap();
    fs::create_dir_all(&data_home).unwrap();

    std::env::set_var("HOME", home.to_str().unwrap());
    std::env::set_var("XDG_CONFIG_HOME", config_home.to_str().unwrap());
    std::env::set_var("XDG_DATA_HOME", data_home.to_str().unwrap());

    let result = f();
    env_state.restore();
    result
}

pub fn assert_tokens_from_fixture(output: &str, fixture_rel_path: &str) {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture_rel_path);
    let fixture = fs::read_to_string(&fixture_path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {}", fixture_path.display(), e));

    let tokens: Vec<&str> = fixture
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();

    let mut cursor = 0usize;
    for token in tokens {
        let remaining = &output[cursor..];
        if let Some(idx) = remaining.find(token) {
            cursor += idx + token.len();
            continue;
        }
        panic!("missing token in order: {token}\noutput:\n{output}");
    }
}

pub fn create_test_agent(agent_id: &str) {
    let prompts_dir = xdg::prompts_dir().unwrap();
    fs::create_dir_all(&prompts_dir).unwrap();
    let prompt_path = prompts_dir.join(format!("{agent_id}.md"));
    fs::write(
        &prompt_path,
        "# Test Prompt\n\nStable prompt for phase1 tests.",
    )
    .unwrap();

    let agents_dir = XdgAgentStorage::new().agents_dir().unwrap();
    fs::create_dir_all(&agents_dir).unwrap();
    let config_path = agents_dir.join(format!("{agent_id}.toml"));

    let mut metadata = HashMap::new();
    metadata.insert("user_prompt_file".to_string(), "Analyze {path}".to_string());
    metadata.insert(
        "user_prompt_directory".to_string(),
        "Analyze directory {path}".to_string(),
    );

    let config = AgentConfig {
        agent_id: agent_id.to_string(),
        role: AgentRole::Writer,
        system_prompt: None,
        system_prompt_path: Some(prompt_path.to_string_lossy().to_string()),
        metadata,
    };

    let toml = toml::to_string_pretty(&config).unwrap();
    fs::write(config_path, toml).unwrap();
}

pub fn create_test_provider(provider_name: &str) {
    let providers_dir = xdg::providers_dir().unwrap();
    fs::create_dir_all(&providers_dir).unwrap();
    let config_path = providers_dir.join(format!("{provider_name}.toml"));

    let provider_config = ProviderConfig {
        provider_name: Some(provider_name.to_string()),
        provider_type: ProviderType::OpenAI,
        model: "gpt-4-test".to_string(),
        api_key: Some("test-api-key".to_string()),
        endpoint: Some("http://127.0.0.1:9".to_string()),
        default_options: merkle::provider::CompletionOptions::default(),
    };

    let toml = toml::to_string_pretty(&provider_config).unwrap();
    fs::write(config_path, toml).unwrap();
}

pub fn latest_session_events(
    runtime: &ProgressRuntime,
    command: &str,
) -> Vec<merkle::telemetry::ProgressEvent> {
    let sessions = runtime.store().list_sessions().unwrap();
    let session = sessions
        .iter()
        .find(|s| s.command == command)
        .unwrap_or_else(|| panic!("missing session for command: {command}"));
    runtime.store().read_events(&session.session_id).unwrap()
}

pub fn create_context_api(workspace_root: &Path) -> ContextApi {
    let store_path = workspace_root.join("store");
    let frames_path = workspace_root.join("frames");

    fs::create_dir_all(&store_path).unwrap();
    fs::create_dir_all(&frames_path).unwrap();

    let node_store = Arc::new(SledNodeRecordStore::new(&store_path).unwrap());
    let frame_storage = Arc::new(FrameStorage::new(&frames_path).unwrap());
    let head_index = Arc::new(parking_lot::RwLock::new(HeadIndex::new()));
    let agent_registry = Arc::new(parking_lot::RwLock::new(AgentRegistry::new()));
    let provider_registry = Arc::new(parking_lot::RwLock::new(ProviderRegistry::new()));
    let lock_manager = Arc::new(NodeLockManager::new());

    ContextApi::with_workspace_root(
        node_store,
        frame_storage,
        head_index,
        agent_registry,
        provider_registry,
        lock_manager,
        workspace_root.to_path_buf(),
    )
}

pub fn create_progress_runtime(db_path: &Path) -> Arc<ProgressRuntime> {
    fs::create_dir_all(db_path).unwrap();
    let db = sled::open(db_path).unwrap();
    Arc::new(ProgressRuntime::new(db).unwrap())
}

pub fn canonical(path: &Path) -> PathBuf {
    merkle::tree::path::canonicalize_path(path).unwrap()
}

pub fn register_writer_agent_in_registry(api: &ContextApi, agent_id: &str) {
    let mut registry = api.agent_registry().write();
    registry.register(AgentIdentity::new(agent_id.to_string(), AgentRole::Writer));
}
