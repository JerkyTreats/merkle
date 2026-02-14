//! Durable sled-backed progress event store.

use std::io;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sled::{Db, Tree};

use crate::error::StorageError;
use crate::progress::event::ProgressEvent;
use crate::progress::session::{now_millis, SessionStatus};

const TREE_SESSIONS: &str = "obs_sessions";
const TREE_EVENTS: &str = "obs_events";
const TREE_META: &str = "obs_session_meta";
const EVENT_KEY_PAD: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub session_id: String,
    pub command: String,
    pub started_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub status: SessionStatus,
    pub status_text: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub next_seq: u64,
    pub latest_status: SessionStatus,
    pub updated_at_ms: u64,
}

#[derive(Clone)]
pub struct ProgressStore {
    db: Db,
    sessions: Tree,
    events: Tree,
    meta: Tree,
}

impl ProgressStore {
    pub fn new(db: Db) -> Result<Self, StorageError> {
        let sessions = db.open_tree(TREE_SESSIONS).map_err(to_storage_io)?;
        let events = db.open_tree(TREE_EVENTS).map_err(to_storage_io)?;
        let meta = db.open_tree(TREE_META).map_err(to_storage_io)?;
        Ok(Self {
            db,
            sessions,
            events,
            meta,
        })
    }

    pub fn shared(db: Db) -> Result<Arc<Self>, StorageError> {
        Ok(Arc::new(Self::new(db)?))
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    pub fn put_session(&self, record: &SessionRecord) -> Result<(), StorageError> {
        let key = record.session_id.as_bytes();
        let value = serde_json::to_vec(record).map_err(to_storage_data)?;
        self.sessions.insert(key, value).map_err(to_storage_io)?;
        Ok(())
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<SessionRecord>, StorageError> {
        let Some(raw) = self
            .sessions
            .get(session_id.as_bytes())
            .map_err(to_storage_io)?
        else {
            return Ok(None);
        };
        let parsed = serde_json::from_slice(&raw).map_err(to_storage_data)?;
        Ok(Some(parsed))
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionRecord>, StorageError> {
        let mut out = Vec::new();
        for result in self.sessions.iter() {
            let (_, value) = result.map_err(to_storage_io)?;
            let rec: SessionRecord = serde_json::from_slice(&value).map_err(to_storage_data)?;
            out.push(rec);
        }
        out.sort_by_key(|s| std::cmp::Reverse(s.started_at_ms));
        Ok(out)
    }

    pub fn put_meta(&self, session_id: &str, meta: &SessionMeta) -> Result<(), StorageError> {
        let value = serde_json::to_vec(meta).map_err(to_storage_data)?;
        self.meta
            .insert(session_id.as_bytes(), value)
            .map_err(to_storage_io)?;
        Ok(())
    }

    pub fn get_meta(&self, session_id: &str) -> Result<Option<SessionMeta>, StorageError> {
        let Some(raw) = self
            .meta
            .get(session_id.as_bytes())
            .map_err(to_storage_io)?
        else {
            return Ok(None);
        };
        let parsed = serde_json::from_slice(&raw).map_err(to_storage_data)?;
        Ok(Some(parsed))
    }

    pub fn append_event(&self, event: &ProgressEvent) -> Result<(), StorageError> {
        let key = encode_event_key(&event.session, event.seq);
        let value = serde_json::to_vec(event).map_err(to_storage_data)?;
        self.events
            .insert(key.as_bytes(), value)
            .map_err(to_storage_io)?;
        Ok(())
    }

    pub fn read_events(&self, session_id: &str) -> Result<Vec<ProgressEvent>, StorageError> {
        self.read_events_after(session_id, 0)
    }

    pub fn read_events_after(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> Result<Vec<ProgressEvent>, StorageError> {
        let prefix = format!("{session_id}:");
        let mut out = Vec::new();
        for result in self.events.scan_prefix(prefix.as_bytes()) {
            let (_, value) = result.map_err(to_storage_io)?;
            let parsed: ProgressEvent = serde_json::from_slice(&value).map_err(to_storage_data)?;
            if parsed.seq > after_seq {
                out.push(parsed);
            }
        }
        out.sort_by_key(|e| e.seq);
        Ok(out)
    }

    pub fn mark_interrupted_sessions(&self) -> Result<usize, StorageError> {
        let mut changed = 0usize;
        let sessions = self.list_sessions()?;
        for mut session in sessions {
            if session.status == SessionStatus::Active {
                session.status = SessionStatus::Interrupted;
                session.status_text = "interrupted".to_string();
                self.put_session(&session)?;
                if let Some(mut meta) = self.get_meta(&session.session_id)? {
                    meta.latest_status = SessionStatus::Interrupted;
                    meta.updated_at_ms = now_millis();
                    self.put_meta(&session.session_id, &meta)?;
                }
                changed += 1;
            }
        }
        Ok(changed)
    }

    pub fn prune_completed(
        &self,
        max_completed: usize,
        max_age_ms: u64,
        now_ms: u64,
    ) -> Result<usize, StorageError> {
        let mut completed: Vec<SessionRecord> = self
            .list_sessions()?
            .into_iter()
            .filter(|s| s.status == SessionStatus::Completed || s.status == SessionStatus::Failed)
            .collect();

        completed.sort_by_key(|s| s.started_at_ms);
        let mut removed = 0usize;

        for session in &completed {
            let ended = session.ended_at_ms.unwrap_or(session.started_at_ms);
            let age = now_ms.saturating_sub(ended);
            if age > max_age_ms {
                self.delete_session(&session.session_id)?;
                removed += 1;
            }
        }

        let mut remaining: Vec<SessionRecord> = self
            .list_sessions()?
            .into_iter()
            .filter(|s| s.status == SessionStatus::Completed || s.status == SessionStatus::Failed)
            .collect();
        remaining.sort_by_key(|s| std::cmp::Reverse(s.started_at_ms));
        if remaining.len() > max_completed {
            for session in remaining.iter().skip(max_completed) {
                self.delete_session(&session.session_id)?;
                removed += 1;
            }
        }

        Ok(removed)
    }

    pub fn flush(&self) -> Result<(), StorageError> {
        self.db.flush().map_err(to_storage_io)?;
        Ok(())
    }

    pub fn encode_event_key(session_id: &str, seq: u64) -> String {
        encode_event_key(session_id, seq)
    }

    pub fn delete_session(&self, session_id: &str) -> Result<(), StorageError> {
        self.sessions
            .remove(session_id.as_bytes())
            .map_err(to_storage_io)?;
        self.meta
            .remove(session_id.as_bytes())
            .map_err(to_storage_io)?;
        let prefix = format!("{session_id}:");
        let keys: Vec<Vec<u8>> = self
            .events
            .scan_prefix(prefix.as_bytes())
            .filter_map(|r| r.ok().map(|(k, _)| k.to_vec()))
            .collect();
        for key in keys {
            self.events.remove(key).map_err(to_storage_io)?;
        }
        Ok(())
    }
}

fn encode_event_key(session_id: &str, seq: u64) -> String {
    format!("{session_id}:{seq:0EVENT_KEY_PAD$}")
}

fn to_storage_io(err: sled::Error) -> StorageError {
    StorageError::IoError(io::Error::new(io::ErrorKind::Other, err.to_string()))
}

fn to_storage_data(err: serde_json::Error) -> StorageError {
    StorageError::IoError(io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::event::ProgressEvent;
    use tempfile::TempDir;

    #[test]
    fn key_encoding_is_lexicographic() {
        let k1 = ProgressStore::encode_event_key("s1", 2);
        let k2 = ProgressStore::encode_event_key("s1", 10);
        assert!(k1 < k2);
    }

    #[test]
    fn write_and_read_events_sorted() {
        let dir = TempDir::new().unwrap();
        let db = sled::open(dir.path()).unwrap();
        let store = ProgressStore::new(db).unwrap();
        let session = "abc";

        let e2 = ProgressEvent {
            ts: "2".to_string(),
            session: session.to_string(),
            seq: 2,
            event_type: "session_ended".to_string(),
            data: serde_json::json!({}),
        };
        let e1 = ProgressEvent {
            ts: "1".to_string(),
            session: session.to_string(),
            seq: 1,
            event_type: "session_started".to_string(),
            data: serde_json::json!({}),
        };
        store.append_event(&e2).unwrap();
        store.append_event(&e1).unwrap();
        let events = store.read_events(session).unwrap();
        assert_eq!(events[0].seq, 1);
        assert_eq!(events[1].seq, 2);
    }
}
