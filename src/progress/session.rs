//! Session lifecycle helpers for progress observability.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;
use tracing::warn;

use crate::error::{ApiError, StorageError};
use crate::progress::bus::ProgressBus;
use crate::progress::ingestor::{EventIngestor, SharedIngestor};
use crate::progress::store::{ProgressStore, SessionMeta, SessionRecord};
use crate::tooling::cli::{AgentCommands, Commands, ContextCommands, ProviderCommands, WorkspaceCommands};

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Active,
    Completed,
    Failed,
    Interrupted,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionStatus::Active => "active",
            SessionStatus::Completed => "completed",
            SessionStatus::Failed => "failed",
            SessionStatus::Interrupted => "interrupted",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PrunePolicy {
    pub max_completed: usize,
    pub max_age_ms: u64,
}

impl Default for PrunePolicy {
    fn default() -> Self {
        Self {
            max_completed: 500,
            max_age_ms: 1000 * 60 * 60 * 24 * 14,
        }
    }
}

#[derive(Clone)]
pub struct ProgressRuntime {
    store: Arc<ProgressStore>,
    bus: ProgressBus,
    ingestor: SharedIngestor,
}

impl ProgressRuntime {
    pub fn new(db: sled::Db) -> Result<Self, StorageError> {
        let store = ProgressStore::shared(db)?;
        let (bus, rx) = ProgressBus::new_pair();
        let ingestor = SharedIngestor::new(EventIngestor::new(store.clone(), rx));
        Ok(Self { store, bus, ingestor })
    }

    pub fn start_command_session(&self, command: String) -> Result<String, ApiError> {
        let session_id = new_session_id();
        let started = now_millis();
        let record = SessionRecord {
            session_id: session_id.clone(),
            command: command.clone(),
            started_at_ms: started,
            ended_at_ms: None,
            status: SessionStatus::Active,
            status_text: SessionStatus::Active.as_str().to_string(),
            error: None,
        };
        self.store.put_session(&record)?;
        let meta = SessionMeta {
            next_seq: 1,
            latest_status: SessionStatus::Active,
            updated_at_ms: started,
        };
        self.store.put_meta(&session_id, &meta)?;

        self.bus
            .emit(
                session_id.clone(),
                "session_started",
                json!({ "command": command }),
            )
            .map_err(to_api_error)?;
        self.ingestor.drain()?;
        self.store.flush()?;
        Ok(session_id)
    }

    pub fn finish_command_session(
        &self,
        session_id: &str,
        success: bool,
        error: Option<String>,
    ) -> Result<(), ApiError> {
        let status = if success {
            SessionStatus::Completed
        } else {
            SessionStatus::Failed
        };
        self.bus
            .emit(
                session_id.to_string(),
                "session_ended",
                json!({ "status": status.as_str(), "error": error }),
            )
            .map_err(to_api_error)?;
        self.ingestor.drain()?;
        let mut record = self
            .store
            .get_session(session_id)?
            .ok_or_else(|| ApiError::StorageError(StorageError::InvalidPath("session record missing".to_string())))?;
        record.status = status;
        record.status_text = status.as_str().to_string();
        record.ended_at_ms = Some(now_millis());
        record.error = error.clone();
        self.store.put_session(&record)?;
        if let Some(mut meta) = self.store.get_meta(session_id)? {
            meta.latest_status = status;
            meta.updated_at_ms = now_millis();
            self.store.put_meta(session_id, &meta)?;
        }
        self.store.flush()?;
        Ok(())
    }

    pub fn emit_event(
        &self,
        session_id: &str,
        event_type: &str,
        data: Value,
    ) -> Result<(), ApiError> {
        self.bus
            .emit(session_id.to_string(), event_type, data)
            .map_err(to_api_error)?;
        self.ingestor.drain()?;
        self.store.flush()?;
        Ok(())
    }

    pub fn emit_event_best_effort(&self, session_id: &str, event_type: &str, data: Value) {
        if let Err(err) = self.emit_event(session_id, event_type, data) {
            warn!(
                session_id = %session_id,
                event_type = %event_type,
                error = %err,
                "failed to emit progress event"
            );
        }
    }

    pub fn mark_interrupted_sessions(&self) -> Result<usize, ApiError> {
        let changed = self.store.mark_interrupted_sessions()?;
        self.store.flush()?;
        Ok(changed)
    }

    pub fn prune(&self, policy: PrunePolicy) -> Result<usize, ApiError> {
        let now = now_millis();
        let pruned = self
            .store
            .prune_completed(policy.max_completed, policy.max_age_ms, now)?;
        self.store.flush()?;
        Ok(pruned)
    }

    pub fn store(&self) -> &ProgressStore {
        &self.store
    }
}

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn new_session_id() -> String {
    let ts = now_millis();
    let pid = std::process::id();
    let seq = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("sess-{ts}-{pid}-{seq}")
}

pub fn command_name(command: &Commands) -> String {
    match command {
        Commands::Synthesize { .. } => "synthesize".to_string(),
        Commands::Regenerate { .. } => "regenerate".to_string(),
        Commands::Scan { .. } => "scan".to_string(),
        Commands::Workspace { command } => format!("workspace.{}", workspace_command_name(command)),
        Commands::Status { .. } => "status".to_string(),
        Commands::Validate => "validate".to_string(),
        Commands::Watch { .. } => "watch".to_string(),
        Commands::Agent { command } => format!("agent.{}", agent_command_name(command)),
        Commands::Provider { command } => format!("provider.{}", provider_command_name(command)),
        Commands::Init { .. } => "init".to_string(),
        Commands::Context { command } => format!("context.{}", context_command_name(command)),
    }
}

fn workspace_command_name(command: &WorkspaceCommands) -> &'static str {
    match command {
        WorkspaceCommands::Status { .. } => "status",
        WorkspaceCommands::Validate { .. } => "validate",
        WorkspaceCommands::Ignore { .. } => "ignore",
        WorkspaceCommands::Delete { .. } => "delete",
        WorkspaceCommands::Restore { .. } => "restore",
        WorkspaceCommands::Compact { .. } => "compact",
        WorkspaceCommands::ListDeleted { .. } => "list_deleted",
    }
}

fn context_command_name(command: &ContextCommands) -> &'static str {
    match command {
        ContextCommands::Generate { .. } => "generate",
        ContextCommands::Get { .. } => "get",
    }
}

fn provider_command_name(command: &ProviderCommands) -> &'static str {
    match command {
        ProviderCommands::Status { .. } => "status",
        ProviderCommands::List { .. } => "list",
        ProviderCommands::Show { .. } => "show",
        ProviderCommands::Create { .. } => "create",
        ProviderCommands::Edit { .. } => "edit",
        ProviderCommands::Remove { .. } => "remove",
        ProviderCommands::Validate { .. } => "validate",
        ProviderCommands::Test { .. } => "test",
    }
}

fn agent_command_name(command: &AgentCommands) -> &'static str {
    match command {
        AgentCommands::Status { .. } => "status",
        AgentCommands::List { .. } => "list",
        AgentCommands::Show { .. } => "show",
        AgentCommands::Create { .. } => "create",
        AgentCommands::Edit { .. } => "edit",
        AgentCommands::Remove { .. } => "remove",
        AgentCommands::Validate { .. } => "validate",
    }
}

fn to_api_error(err: std::sync::mpsc::SendError<crate::progress::event::ProgressEnvelope>) -> ApiError {
    ApiError::StorageError(StorageError::IoError(std::io::Error::new(
        std::io::ErrorKind::Other,
        err.to_string(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_are_unique() {
        let a = new_session_id();
        let b = new_session_id();
        assert_ne!(a, b);
    }
}
