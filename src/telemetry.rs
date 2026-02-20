//! Telemetry domain: events, sessions, routing, sinks, and emission.

mod types;

pub mod emission;
pub mod events;
pub mod facade;
pub mod routing;
pub mod sessions;
pub mod sinks;
pub mod summary;

pub use events::{
    ProgressEvent, ProviderLifecycleEventData, QueueEventData, QueueStatsEventData,
    SessionEndedData, SessionStartedData, SummaryEventData,
};
pub use sessions::policy::{PrunePolicy, SessionStatus};
pub use sessions::ProgressRuntime;
pub use types::{new_session_id, now_millis};
