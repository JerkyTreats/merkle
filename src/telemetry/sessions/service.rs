//! Session lifecycle service. No CLI dependency; takes command_name as String.

use std::sync::Arc;

use serde_json::json;
use serde_json::Value;
use tracing::warn;

use crate::error::{ApiError, StorageError};
use crate::telemetry::events::ProgressEnvelope;
use crate::telemetry::routing::bus::ProgressBus;
use crate::telemetry::routing::ingestor::{EventIngestor, SharedIngestor};
use crate::telemetry::sessions::policy::{PrunePolicy, SessionStatus};
use crate::telemetry::sinks::store::{ProgressStore, SessionMeta, SessionRecord};
use crate::telemetry::types::{new_session_id, now_millis};

/// Runtime for session lifecycle and event emission. Holds store, bus, ingestor.
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
        Ok(Self {
            store,
            bus,
            ingestor,
        })
    }

    pub fn start_command_session(&self, command_name: String) -> Result<String, ApiError> {
        let session_id = new_session_id();
        let started = now_millis();
        let record = SessionRecord {
            session_id: session_id.clone(),
            command: command_name.clone(),
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
                json!({ "command": command_name }),
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
        let mut record = self.store.get_session(session_id)?.ok_or_else(|| {
            ApiError::StorageError(StorageError::InvalidPath(
                "session record missing".to_string(),
            ))
        })?;
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

fn to_api_error(err: std::sync::mpsc::SendError<ProgressEnvelope>) -> ApiError {
    ApiError::StorageError(StorageError::IoError(std::io::Error::new(
        std::io::ErrorKind::Other,
        err.to_string(),
    )))
}
