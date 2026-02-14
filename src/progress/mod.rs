//! Progress observability primitives.

pub mod bus;
pub mod event;
pub mod ingestor;
pub mod session;
pub mod store;

pub use bus::ProgressBus;
pub use event::{
    ProgressEnvelope,
    ProgressEvent,
    ProviderLifecycleEventData,
    QueueEventData,
    QueueStatsEventData,
    SessionEndedData,
    SessionStartedData,
    SummaryEventData,
};
pub use ingestor::EventIngestor;
pub use session::{
    command_name,
    now_millis,
    new_session_id,
    PrunePolicy,
    ProgressRuntime,
    SessionStatus,
};
pub use store::{ProgressStore, SessionMeta, SessionRecord};
