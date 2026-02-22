//! Public view types for context query: ContextView, ContextViewBuilder, NodeContext.
//! Owned by context domain; api re-exports for compatibility.

use super::view_policy::{FrameFilter, OrderingPolicy, ViewPolicy};
use crate::context::frame::Frame;
use crate::store::NodeRecord;
use crate::types::NodeID;
use serde::{Deserialize, Serialize};

/// Context view policy for frame selection
///
/// Wraps ViewPolicy to provide a clean API interface.
/// This is the policy-driven view that determines which frames are selected
/// and how they are ordered.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextView {
    /// Maximum number of frames to return
    pub max_frames: usize,
    /// Ordering policy for frame selection
    pub ordering: OrderingPolicy,
    /// Filters to apply before ordering
    pub filters: Vec<FrameFilter>,
}

impl From<ViewPolicy> for ContextView {
    fn from(policy: ViewPolicy) -> Self {
        ContextView {
            max_frames: policy.max_frames,
            ordering: policy.ordering,
            filters: policy.filters,
        }
    }
}

impl From<ContextView> for ViewPolicy {
    fn from(view: ContextView) -> Self {
        ViewPolicy {
            max_frames: view.max_frames,
            ordering: view.ordering,
            filters: view.filters,
        }
    }
}

impl ContextView {
    /// Create a new builder for constructing ContextView
    ///
    /// Provides a fluent API for building context views.
    ///
    /// # Example
    /// ```rust
    /// use merkle::context::query::view::ContextView;
    ///
    /// let view = ContextView::builder()
    ///     .max_frames(20)
    ///     .recent()
    ///     .by_type("analysis")
    ///     .by_agent("agent-1")
    ///     .build();
    /// ```
    pub fn builder() -> ContextViewBuilder {
        ContextViewBuilder::default()
    }
}

/// Builder for constructing ContextView with a fluent API
#[derive(Debug, Default)]
pub struct ContextViewBuilder {
    max_frames: Option<usize>,
    ordering: Option<OrderingPolicy>,
    filters: Vec<FrameFilter>,
}

impl ContextViewBuilder {
    /// Set maximum number of frames to return
    ///
    /// Defaults to 100 if not specified.
    pub fn max_frames(mut self, n: usize) -> Self {
        self.max_frames = Some(n);
        self
    }

    /// Order by recency (most recent first)
    pub fn recent(mut self) -> Self {
        self.ordering = Some(OrderingPolicy::Recency);
        self
    }

    /// Order by frame type (lexicographic)
    pub fn by_type_ordering(mut self) -> Self {
        self.ordering = Some(OrderingPolicy::Type);
        self
    }

    /// Order by agent ID (lexicographic)
    pub fn by_agent_ordering(mut self) -> Self {
        self.ordering = Some(OrderingPolicy::Agent);
        self
    }

    /// Filter by frame type
    pub fn by_type(mut self, frame_type: impl Into<String>) -> Self {
        self.filters.push(FrameFilter::ByType(frame_type.into()));
        self
    }

    /// Filter by agent ID
    pub fn by_agent(mut self, agent_id: impl Into<String>) -> Self {
        self.filters.push(FrameFilter::ByAgent(agent_id.into()));
        self
    }

    /// Build the ContextView
    ///
    /// Uses default values for any fields not explicitly set:
    /// - max_frames: 100
    /// - ordering: Recency
    pub fn build(self) -> ContextView {
        ContextView {
            max_frames: self.max_frames.unwrap_or(100),
            ordering: self.ordering.unwrap_or(OrderingPolicy::Recency),
            filters: self.filters,
        }
    }
}

/// Node context response
///
/// Contains the node record and selected frames based on the context view policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeContext {
    /// The NodeID for this context
    pub node_id: NodeID,
    /// The node record (metadata, path, children, etc.)
    pub node_record: NodeRecord,
    /// Selected frames based on the view policy
    pub frames: Vec<Frame>,
    /// Total frame count (may exceed view limit)
    pub frame_count: usize,
}

impl NodeContext {
    /// Get all frame contents as UTF-8 strings
    ///
    /// Filters out frames with invalid UTF-8 content.
    pub fn text_contents(&self) -> Vec<String> {
        self.frames
            .iter()
            .filter_map(|f| f.text_content().ok())
            .collect()
    }

    /// Get concatenated text content with separator
    ///
    /// Combines all valid UTF-8 frame contents into a single string,
    /// separated by the specified separator.
    pub fn combined_text(&self, separator: &str) -> String {
        self.text_contents().join(separator)
    }

    /// Get content slices filtered by frame type
    ///
    /// Returns raw byte slices for frames matching the specified type.
    pub fn content_by_type(&self, frame_type: &str) -> Vec<&[u8]> {
        self.frames
            .iter()
            .filter(|f| f.is_type(frame_type))
            .map(|f| f.content.as_slice())
            .collect()
    }

    /// Parse frames as JSON
    ///
    /// Attempts to parse each frame's content as JSON into the specified type.
    /// Returns a vector of results, allowing callers to handle parsing errors per frame.
    pub fn json_frames<T>(&self) -> Vec<Result<T, serde_json::Error>>
    where
        T: serde::de::DeserializeOwned,
    {
        self.frames.iter().map(|f| f.json_content()).collect()
    }

    /// Get most recent frame of specific type
    ///
    /// Returns the frame with the latest timestamp that matches the specified type.
    pub fn latest_frame_of_type(&self, frame_type: &str) -> Option<&Frame> {
        self.frames
            .iter()
            .filter(|f| f.is_type(frame_type))
            .max_by_key(|f| f.timestamp)
    }

    /// Get all frames from specific agent
    ///
    /// Returns all frames where the agent_id in metadata matches the specified agent.
    pub fn frames_by_agent(&self, agent_id: &str) -> Vec<&Frame> {
        self.frames
            .iter()
            .filter(|f| f.agent_id().map(|id| id == agent_id).unwrap_or(false))
            .collect()
    }

    /// Iterator over frames
    ///
    /// Provides an iterator over all frames in the context.
    pub fn frames_iter(&self) -> impl Iterator<Item = &Frame> {
        self.frames.iter()
    }

    /// Iterator over text content
    ///
    /// Provides an iterator over valid UTF-8 text content from frames.
    /// Filters out frames with invalid UTF-8.
    pub fn text_iter(&self) -> impl Iterator<Item = String> + '_ {
        self.frames.iter().filter_map(|f| f.text_content().ok())
    }

    /// Get frames filtered by type
    ///
    /// Returns all frames matching the specified type.
    pub fn filter_by_type(&self, frame_type: &str) -> Vec<&Frame> {
        self.frames
            .iter()
            .filter(|f| f.is_type(frame_type))
            .collect()
    }
}
