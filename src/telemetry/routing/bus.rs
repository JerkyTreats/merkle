//! In-process event bus for telemetry events.

use std::sync::mpsc::{channel, Receiver, Sender};

use serde_json::Value;

use crate::telemetry::events::ProgressEnvelope;

#[derive(Clone)]
pub struct ProgressBus {
    sender: Sender<ProgressEnvelope>,
}

impl ProgressBus {
    pub fn new_pair() -> (Self, Receiver<ProgressEnvelope>) {
        let (sender, receiver) = channel();
        (Self { sender }, receiver)
    }

    pub fn emit(
        &self,
        session: impl Into<String>,
        event_type: impl Into<String>,
        data: Value,
    ) -> Result<(), std::sync::mpsc::SendError<ProgressEnvelope>> {
        let envelope = ProgressEnvelope::with_now(session, event_type, data);
        self.sender.send(envelope)
    }
}
