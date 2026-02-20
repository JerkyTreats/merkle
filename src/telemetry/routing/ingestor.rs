//! Event ingestion and sequence assignment.

use std::sync::{mpsc::Receiver, Arc, Mutex};

use crate::error::StorageError;
use crate::telemetry::events::{ProgressEnvelope, ProgressEvent};
use crate::telemetry::sessions::policy::SessionStatus;
use crate::telemetry::sinks::store::{ProgressStore, SessionMeta};
use crate::telemetry::types::now_millis;

pub struct EventIngestor {
    store: Arc<ProgressStore>,
    receiver: Receiver<ProgressEnvelope>,
}

impl EventIngestor {
    pub fn new(store: Arc<ProgressStore>, receiver: Receiver<ProgressEnvelope>) -> Self {
        Self { store, receiver }
    }

    pub fn ingest_pending(&mut self) -> Result<usize, StorageError> {
        let mut count = 0usize;
        while let Ok(envelope) = self.receiver.try_recv() {
            self.ingest_one(envelope)?;
            count += 1;
        }
        Ok(count)
    }

    fn ingest_one(&self, envelope: ProgressEnvelope) -> Result<(), StorageError> {
        let mut meta = self
            .store
            .get_meta(&envelope.session)?
            .unwrap_or(SessionMeta {
                next_seq: 1,
                latest_status: SessionStatus::Active,
                updated_at_ms: now_millis(),
            });

        let seq = meta.next_seq;
        let event = ProgressEvent::from_envelope(envelope, seq);
        self.store.append_event(&event)?;
        meta.next_seq += 1;
        meta.updated_at_ms = now_millis();
        self.store.put_meta(&event.session, &meta)?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct SharedIngestor(Arc<Mutex<EventIngestor>>);

impl SharedIngestor {
    pub fn new(inner: EventIngestor) -> Self {
        Self(Arc::new(Mutex::new(inner)))
    }

    pub fn drain(&self) -> Result<usize, StorageError> {
        let mut guard = self.0.lock().expect("ingestor lock poisoned");
        guard.ingest_pending()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::routing::bus::ProgressBus;

    #[test]
    fn sequence_assignment_is_monotonic() {
        let dir = tempfile::TempDir::new().unwrap();
        let db = sled::open(dir.path()).unwrap();
        let store = ProgressStore::shared(db).unwrap();
        let (bus, rx) = ProgressBus::new_pair();
        let mut ingestor = EventIngestor::new(store.clone(), rx);
        bus.emit("s1", "session_started", serde_json::json!({}))
            .unwrap();
        bus.emit("s1", "session_ended", serde_json::json!({}))
            .unwrap();
        ingestor.ingest_pending().unwrap();
        let events = store.read_events("s1").unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].seq, 1);
        assert_eq!(events[1].seq, 2);
    }
}
