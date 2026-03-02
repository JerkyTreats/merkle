//! Frame Generation Queue
//!
//! Batch queue system for automatically generating context frames using LLM providers.
//! Handles large-scale operations efficiently through batching, rate limiting, and concurrent processing.

use crate::api::{ContextApi, ContextView};
use crate::agent::profile::prompt_contract::PromptContract;
use crate::context::frame::{Basis, Frame};
use crate::error::ApiError;
use crate::metadata::frame_types::FrameMetadata;
use crate::metadata::frame_write_contract::{build_generated_metadata, validate_frame_metadata};
use crate::provider::ChatMessage;
use crate::store::NodeRecord;
use crate::telemetry::{
    ProgressRuntime, ProviderLifecycleEventData, QueueEventData, QueueStatsEventData,
};
use crate::types::{FrameID, NodeID};
use hex;
use parking_lot::RwLock;
use serde_json::json;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, Mutex, Notify, Semaphore};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

type GeneratedMetadataBuilder =
    dyn Fn(&str, &str, &str, &str, &str) -> FrameMetadata + Send + Sync;

/// Priority level for generation requests
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low = 0,    // Existing files during initial scan
    Normal = 1, // Default priority
    High = 2,   // New files in watch mode
    Urgent = 3, // User-initiated requests
}

/// Request ID for tracking completion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestId(u64);

impl RequestId {
    /// Generate the next request ID (for internal use and testing)
    pub fn next() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        RequestId(COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

#[derive(Clone)]
pub struct QueueEventContext {
    pub session_id: String,
    pub progress: Arc<ProgressRuntime>,
}

#[derive(Debug, Clone, Default)]
pub struct GenerationRequestOptions {
    pub force: bool,
    pub plan_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RequestIdentity {
    node_id: NodeID,
    agent_id: String,
    frame_type: String,
}

impl RequestIdentity {
    fn new(node_id: NodeID, agent_id: &str, frame_type: &str) -> Self {
        Self {
            node_id,
            agent_id: agent_id.to_string(),
            frame_type: frame_type.to_string(),
        }
    }

    fn from_request(request: &GenerationRequest) -> Self {
        Self::new(request.node_id, &request.agent_id, &request.frame_type)
    }
}

#[derive(Debug)]
struct DedupeEntry {
    request_id: RequestId,
    waiters: Vec<oneshot::Sender<Result<FrameID, ApiError>>>,
}

impl DedupeEntry {
    fn new(request_id: RequestId) -> Self {
        Self {
            request_id,
            waiters: Vec::new(),
        }
    }
}

/// Generation request
#[derive(Debug)]
pub struct GenerationRequest {
    /// Request ID for tracking completion
    pub request_id: RequestId,
    /// NodeID to generate frame for
    pub node_id: NodeID,
    /// Agent ID that will generate the frame
    pub agent_id: String,
    /// Provider name to use for generation
    pub provider_name: String,
    /// Frame type to generate
    pub frame_type: String,
    /// Priority level (higher = more important)
    pub priority: Priority,
    /// Number of retry attempts made
    pub retry_count: usize,
    /// Timestamp when request was created
    pub created_at: Instant,
    /// Optional completion channel for sync requests (not cloneable)
    pub completion_tx: Option<oneshot::Sender<Result<FrameID, ApiError>>>,
    /// Additional request execution options
    pub options: GenerationRequestOptions,
}

impl Clone for GenerationRequest {
    fn clone(&self) -> Self {
        Self {
            request_id: self.request_id,
            node_id: self.node_id,
            agent_id: self.agent_id.clone(),
            provider_name: self.provider_name.clone(),
            frame_type: self.frame_type.clone(),
            priority: self.priority,
            retry_count: self.retry_count,
            created_at: self.created_at,
            completion_tx: None, // Don't clone completion channel
            options: self.options.clone(),
        }
    }
}

impl PartialEq for GenerationRequest {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id
    }
}

impl Eq for GenerationRequest {}

impl Ord for GenerationRequest {
    /// Order by priority (higher first), then by creation time (older first for same priority)
    /// BinaryHeap is a max-heap, so higher priority should compare as Greater
    /// For same priority, older items (smaller timestamp) should be Greater (processed first)
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let self_plan_rank = if self.options.plan_id.is_some() { 1 } else { 0 };
        let other_plan_rank = if other.options.plan_id.is_some() {
            1
        } else {
            0
        };
        match self_plan_rank.cmp(&other_plan_rank) {
            std::cmp::Ordering::Equal => {}
            ordering => return ordering,
        }

        match self.priority.cmp(&other.priority) {
            std::cmp::Ordering::Equal => {
                // Older items (smaller timestamp) should be Greater (processed first)
                self.created_at.cmp(&other.created_at).reverse()
            }
            // Higher priority (larger enum value) should be Greater
            ordering => ordering,
        }
    }
}

impl PartialOrd for GenerationRequest {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Configuration for the generation queue
#[derive(Debug, Clone)]
pub struct GenerationConfig {
    /// Maximum concurrent generations per agent
    pub max_concurrent_per_agent: usize,
    /// Batch size for processing requests
    pub batch_size: usize,
    /// Maximum retry attempts per request
    pub max_retry_attempts: usize,
    /// Delay between retries (milliseconds)
    pub retry_delay_ms: u64,
    /// Rate limit: minimum delay between requests per agent (milliseconds)
    pub rate_limit_ms: Option<u64>,
    /// Maximum queue size (prevents memory exhaustion)
    pub max_queue_size: usize,
    /// Number of worker tasks per agent
    pub workers_per_agent: usize,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            max_concurrent_per_agent: 3,
            batch_size: 50,
            max_retry_attempts: 3,
            retry_delay_ms: 1000,
            rate_limit_ms: Some(100), // 100ms between requests per agent
            max_queue_size: 10000,
            workers_per_agent: 2,
        }
    }
}

/// Queue statistics
#[derive(Debug, Clone, Default)]
pub struct QueueStats {
    /// Number of pending requests
    pub pending: usize,
    /// Number of requests currently being processed
    pub processing: usize,
    /// Number of completed requests
    pub completed: usize,
    /// Number of failed requests
    pub failed: usize,
}

/// Per-agent rate limiter
struct AgentRateLimiter {
    semaphore: Arc<Semaphore>,
    last_request: Arc<RwLock<HashMap<String, Instant>>>,
    min_delay: Option<Duration>,
}

impl AgentRateLimiter {
    fn new(max_concurrent: usize, min_delay_ms: Option<u64>) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            last_request: Arc::new(RwLock::new(HashMap::new())),
            min_delay: min_delay_ms.map(Duration::from_millis),
        }
    }

    async fn acquire(&self, agent_id: &str) -> Result<tokio::sync::SemaphorePermit<'_>, ApiError> {
        // Acquire semaphore (concurrency limit)
        let permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| ApiError::ProviderRateLimit("Semaphore closed".to_string()))?;

        // Check rate limit delay
        if let Some(min_delay) = self.min_delay {
            let sleep_duration = {
                let last = self.last_request.read();
                if let Some(last_time) = last.get(agent_id) {
                    let elapsed = last_time.elapsed();
                    if elapsed < min_delay {
                        Some(min_delay - elapsed)
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            // Sleep if needed (after dropping the guard)
            if let Some(duration) = sleep_duration {
                sleep(duration).await;
            }

            // Update last request time
            {
                let mut last = self.last_request.write();
                last.insert(agent_id.to_string(), Instant::now());
            }
        }

        Ok(permit)
    }
}

/// Frame generation queue
pub struct FrameGenerationQueue {
    /// Pending requests (priority queue using BinaryHeap)
    queue: Arc<Mutex<BinaryHeap<GenerationRequest>>>,
    /// Notifier to wake workers when new items are enqueued
    notify: Arc<Notify>,
    /// Active worker tasks
    workers: Arc<RwLock<Vec<tokio::task::JoinHandle<()>>>>,
    /// Configuration
    config: GenerationConfig,
    /// API for frame operations
    api: Arc<ContextApi>,
    /// Rate limiters per agent
    rate_limiters: Arc<RwLock<HashMap<String, AgentRateLimiter>>>,
    /// Running state
    running: Arc<RwLock<bool>>,
    /// Statistics
    stats: Arc<RwLock<QueueStats>>,
    /// Optional observability context for queue and provider lifecycle events
    event_context: Option<QueueEventContext>,
    /// Index of active requests (queued or in-flight) by dedupe identity
    dedupe_index: Arc<Mutex<HashMap<RequestIdentity, DedupeEntry>>>,
    /// Builder for generated frame metadata.
    metadata_builder: Arc<GeneratedMetadataBuilder>,
}

impl FrameGenerationQueue {
    const FILE_CONTEXT_MAX_BYTES: usize = 128 * 1024;

    /// Create a new generation queue
    pub fn new(api: Arc<ContextApi>, config: GenerationConfig) -> Self {
        Self::with_event_context(api, config, None)
    }

    pub fn with_event_context(
        api: Arc<ContextApi>,
        config: GenerationConfig,
        event_context: Option<QueueEventContext>,
    ) -> Self {
        Self::with_custom_metadata_builder(
            api,
            config,
            event_context,
            build_generated_metadata,
        )
    }

    /// Create a queue with a custom generated metadata builder.
    pub fn with_custom_metadata_builder<F>(
        api: Arc<ContextApi>,
        config: GenerationConfig,
        event_context: Option<QueueEventContext>,
        metadata_builder: F,
    ) -> Self
    where
        F: Fn(&str, &str, &str, &str, &str) -> FrameMetadata + Send + Sync + 'static,
    {
        Self {
            queue: Arc::new(Mutex::new(BinaryHeap::new())),
            notify: Arc::new(Notify::new()),
            workers: Arc::new(RwLock::new(Vec::new())),
            config,
            api,
            rate_limiters: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
            stats: Arc::new(RwLock::new(QueueStats::default())),
            event_context,
            dedupe_index: Arc::new(Mutex::new(HashMap::new())),
            metadata_builder: Arc::new(metadata_builder),
        }
    }

    /// Enqueue a generation request (async - returns immediately)
    pub async fn enqueue(
        &self,
        node_id: NodeID,
        agent_id: String,
        provider_name: String,
        frame_type: Option<String>,
        priority: Priority,
    ) -> Result<RequestId, ApiError> {
        let mut queue = self.queue.lock().await;
        let mut dedupe = self.dedupe_index.lock().await;
        let resolved_frame_type = frame_type
            .clone()
            .unwrap_or_else(|| format!("context-{}", agent_id));
        let identity = RequestIdentity::new(node_id, &agent_id, &resolved_frame_type);

        if let Some(existing_entry) = dedupe.get(&identity) {
            let existing_id = existing_entry.request_id;
            self.emit_queue_event(
                "request_deduplicated",
                QueueEventData {
                    node_id: hex::encode(node_id),
                    agent_id,
                    provider_name,
                    frame_type: resolved_frame_type,
                    request_id: Some(existing_id.as_u64()),
                    retry_count: None,
                    duration_ms: None,
                },
            );
            return Ok(existing_id);
        }

        // Check queue size limit
        if queue.len() >= self.config.max_queue_size {
            warn!(
                queue_size = queue.len(),
                max_size = self.config.max_queue_size,
                "Generation queue is full, dropping request"
            );
            return Err(ApiError::ConfigError(
                "Generation queue is full".to_string(),
            ));
        }

        let request_id = RequestId::next();

        // Use provided frame_type or default to "context-{agent_id}"
        let frame_type = resolved_frame_type;

        let request = GenerationRequest {
            request_id,
            node_id,
            agent_id: agent_id.clone(),
            provider_name: provider_name.clone(),
            frame_type: frame_type.clone(),
            priority,
            retry_count: 0,
            created_at: Instant::now(),
            completion_tx: None,
            options: GenerationRequestOptions::default(),
        };

        // Push to priority queue (BinaryHeap maintains max-heap property)
        queue.push(request);
        dedupe.insert(identity, DedupeEntry::new(request_id));

        // Update stats
        {
            let mut stats = self.stats.write();
            stats.pending += 1;
        }
        self.emit_queue_stats_event();

        // Notify workers that a new item is available
        self.notify.notify_one();
        let queue_size = queue.len();
        drop(dedupe);
        drop(queue);

        debug!(
            request_id = ?request_id,
            node_id = %hex::encode(node_id),
            agent_id = %agent_id,
            provider_name = %provider_name,
            priority = ?priority,
            queue_size = queue_size,
            "Enqueued generation request"
        );
        self.emit_queue_event(
            "request_enqueued",
            QueueEventData {
                node_id: hex::encode(node_id),
                agent_id: agent_id.clone(),
                provider_name: provider_name.clone(),
                frame_type: frame_type.clone(),
                request_id: Some(request_id.as_u64()),
                retry_count: Some(0),
                duration_ms: None,
            },
        );

        Ok(request_id)
    }

    /// Enqueue a generation request and wait for completion (sync)
    pub async fn enqueue_and_wait(
        &self,
        node_id: NodeID,
        agent_id: String,
        provider_name: String,
        frame_type: Option<String>,
        priority: Priority,
        timeout: Option<Duration>,
    ) -> Result<FrameID, ApiError> {
        self.enqueue_and_wait_with_options(
            node_id,
            agent_id,
            provider_name,
            frame_type,
            priority,
            timeout,
            GenerationRequestOptions::default(),
        )
        .await
    }

    pub async fn enqueue_and_wait_with_options(
        &self,
        node_id: NodeID,
        agent_id: String,
        provider_name: String,
        frame_type: Option<String>,
        priority: Priority,
        timeout: Option<Duration>,
        options: GenerationRequestOptions,
    ) -> Result<FrameID, ApiError> {
        let (tx, rx) = oneshot::channel();
        let mut queue = self.queue.lock().await;
        let mut dedupe = self.dedupe_index.lock().await;

        let resolved_frame_type = frame_type.unwrap_or_else(|| format!("context-{}", agent_id));
        let identity = RequestIdentity::new(node_id, &agent_id, &resolved_frame_type);

        if let Some(existing_entry) = dedupe.get_mut(&identity) {
            existing_entry.waiters.push(tx);
            let existing_id = existing_entry.request_id;
            drop(dedupe);
            drop(queue);
            self.emit_queue_event(
                "request_deduplicated",
                QueueEventData {
                    node_id: hex::encode(node_id),
                    agent_id,
                    provider_name,
                    frame_type: resolved_frame_type,
                    request_id: Some(existing_id.as_u64()),
                    retry_count: None,
                    duration_ms: None,
                },
            );
            return self.wait_for_generation_completion(rx, timeout).await;
        }

        if !options.force {
            if let Some(existing_head) = self.api.get_head(&node_id, &resolved_frame_type)? {
                drop(dedupe);
                drop(queue);
                self.emit_queue_event(
                    "request_deduplicated",
                    QueueEventData {
                        node_id: hex::encode(node_id),
                        agent_id,
                        provider_name,
                        frame_type: resolved_frame_type,
                        request_id: None,
                        retry_count: None,
                        duration_ms: None,
                    },
                );
                return Ok(existing_head);
            }
        }

        // Check queue size limit
        if queue.len() >= self.config.max_queue_size {
            warn!(
                queue_size = queue.len(),
                max_size = self.config.max_queue_size,
                "Generation queue is full, dropping request"
            );
            return Err(ApiError::ConfigError(
                "Generation queue is full".to_string(),
            ));
        }

        let request_id = RequestId::next();
        let frame_type = resolved_frame_type;

        let request = GenerationRequest {
            request_id,
            node_id,
            agent_id: agent_id.clone(),
            provider_name: provider_name.clone(),
            frame_type: frame_type.clone(),
            priority,
            retry_count: 0,
            created_at: Instant::now(),
            completion_tx: None,
            options,
        };

        // Push to priority queue (BinaryHeap maintains max-heap property)
        queue.push(request);
        let mut entry = DedupeEntry::new(request_id);
        entry.waiters.push(tx);
        dedupe.insert(identity, entry);

        // Update stats
        {
            let mut stats = self.stats.write();
            stats.pending += 1;
        }
        self.emit_queue_stats_event();

        // Notify workers that a new item is available
        self.notify.notify_one();
        drop(dedupe);
        drop(queue);

        debug!(
            request_id = ?request_id,
            node_id = %hex::encode(node_id),
            agent_id = %agent_id,
            provider_name = %provider_name,
            priority = ?priority,
            "Enqueued sync generation request"
        );
        self.emit_queue_event(
            "request_enqueued",
            QueueEventData {
                node_id: hex::encode(node_id),
                agent_id: agent_id.clone(),
                provider_name: provider_name.clone(),
                frame_type: frame_type.clone(),
                request_id: Some(request_id.as_u64()),
                retry_count: Some(0),
                duration_ms: None,
            },
        );

        self.wait_for_generation_completion(rx, timeout).await
    }

    /// Enqueue multiple requests (batch enqueue)
    pub async fn enqueue_batch(
        &self,
        requests: Vec<(NodeID, String, String, Option<String>, Priority)>,
    ) -> Result<Vec<RequestId>, ApiError> {
        let mut queue = self.queue.lock().await;
        let mut dedupe = self.dedupe_index.lock().await;
        let mut request_ids: Vec<RequestId> = Vec::new();
        let mut new_requests = Vec::new();
        let mut staged = HashMap::new();
        let mut enqueue_events = Vec::new();

        for (node_id, agent_id, provider_name, frame_type, priority) in requests {
            let frame_type = frame_type.unwrap_or_else(|| format!("context-{}", agent_id));
            let identity = RequestIdentity::new(node_id, &agent_id, &frame_type);

            if let Some(existing_id) = staged.get(&identity) {
                request_ids.push(*existing_id);
                self.emit_queue_event(
                    "request_deduplicated",
                    QueueEventData {
                        node_id: hex::encode(node_id),
                        agent_id,
                        provider_name,
                        frame_type,
                        request_id: Some(existing_id.as_u64()),
                        retry_count: None,
                        duration_ms: None,
                    },
                );
                continue;
            }

            if let Some(existing_entry) = dedupe.get(&identity) {
                request_ids.push(existing_entry.request_id);
                self.emit_queue_event(
                    "request_deduplicated",
                    QueueEventData {
                        node_id: hex::encode(node_id),
                        agent_id,
                        provider_name,
                        frame_type,
                        request_id: Some(existing_entry.request_id.as_u64()),
                        retry_count: None,
                        duration_ms: None,
                    },
                );
                continue;
            }

            let request_id = RequestId::next();
            let request = GenerationRequest {
                request_id,
                node_id,
                agent_id: agent_id.clone(),
                provider_name: provider_name.clone(),
                frame_type: frame_type.clone(),
                priority,
                retry_count: 0,
                created_at: Instant::now(),
                completion_tx: None,
                options: GenerationRequestOptions::default(),
            };
            request_ids.push(request_id);
            staged.insert(identity.clone(), request_id);
            new_requests.push((identity, request));
        }

        // Check if batch would exceed queue size
        if queue.len() + new_requests.len() > self.config.max_queue_size {
            warn!(
                queue_size = queue.len(),
                batch_size = new_requests.len(),
                max_size = self.config.max_queue_size,
                "Batch would exceed queue size limit"
            );
            return Err(ApiError::ConfigError(
                "Batch would exceed generation queue size limit".to_string(),
            ));
        }

        let new_count = new_requests.len();
        for (identity, request) in new_requests {
            let request_id = request.request_id;
            enqueue_events.push(QueueEventData {
                node_id: hex::encode(request.node_id),
                agent_id: request.agent_id.clone(),
                provider_name: request.provider_name.clone(),
                frame_type: request.frame_type.clone(),
                request_id: Some(request_id.as_u64()),
                retry_count: Some(request.retry_count),
                duration_ms: None,
            });
            queue.push(request);
            dedupe.insert(identity, DedupeEntry::new(request_id));
        }

        let batch_size = new_count;

        // Update stats
        {
            let mut stats = self.stats.write();
            stats.pending += batch_size;
        }
        self.emit_queue_stats_event();

        // Notify workers (multiple times for multiple items)
        let notify_count = batch_size.min(self.config.workers_per_agent);
        for _ in 0..notify_count {
            self.notify.notify_one();
        }

        drop(dedupe);
        drop(queue);

        debug!(
            batch_size = batch_size,
            "Enqueued batch of generation requests"
        );

        for payload in enqueue_events {
            self.emit_queue_event("request_enqueued", payload);
        }

        Ok(request_ids)
    }

    /// Start background workers
    pub fn start(&self) -> Result<(), ApiError> {
        let mut running = self.running.write();
        if *running {
            return Ok(()); // Already running
        }
        *running = true;
        drop(running);

        // Get unique agent IDs from queue to determine worker count
        // We'll start workers that will process requests for any agent
        let worker_count = self.config.workers_per_agent;

        let mut workers = self.workers.write();
        for i in 0..worker_count {
            let queue = Arc::clone(&self.queue);
            let notify = Arc::clone(&self.notify);
            let api = Arc::clone(&self.api);
            let config = self.config.clone();
            let rate_limiters = Arc::clone(&self.rate_limiters);
            let running = Arc::clone(&self.running);
            let stats = Arc::clone(&self.stats);
            let event_context = self.event_context.clone();
            let dedupe_index = Arc::clone(&self.dedupe_index);
            let metadata_builder = Arc::clone(&self.metadata_builder);

            let handle = tokio::spawn(async move {
                Self::worker_loop(
                    i,
                    queue,
                    notify,
                    api,
                    config,
                    rate_limiters,
                    running,
                    stats,
                    event_context,
                    dedupe_index,
                    metadata_builder,
                )
                .await;
            });

            workers.push(handle);
        }

        info!(
            worker_count = workers.len(),
            "Started frame generation queue workers"
        );

        Ok(())
    }

    /// Stop background workers (graceful shutdown)
    pub async fn stop(&self) -> Result<(), ApiError> {
        let mut running = self.running.write();
        if !*running {
            return Ok(()); // Already stopped
        }
        *running = false;
        drop(running);

        // Wait for all workers to finish
        let workers = std::mem::take(&mut *self.workers.write());
        for handle in workers {
            let _ = handle.await;
        }

        info!("Stopped frame generation queue workers");
        Ok(())
    }

    /// Get queue statistics
    pub fn stats(&self) -> QueueStats {
        self.stats.read().clone()
    }

    /// Wait for queue to drain (all requests processed)
    pub async fn wait_for_completion(&self, timeout: Option<Duration>) -> Result<(), ApiError> {
        let start = Instant::now();
        loop {
            let queue = self.queue.lock().await;
            let stats = self.stats.read();

            if queue.is_empty() && stats.processing == 0 {
                return Ok(());
            }

            if let Some(timeout) = timeout {
                if start.elapsed() >= timeout {
                    return Err(ApiError::ConfigError(
                        "Timeout waiting for queue to drain".to_string(),
                    ));
                }
            }

            drop(queue);
            drop(stats);
            sleep(Duration::from_millis(100)).await;
        }
    }

    async fn wait_for_generation_completion(
        &self,
        receiver: oneshot::Receiver<Result<FrameID, ApiError>>,
        timeout: Option<Duration>,
    ) -> Result<FrameID, ApiError> {
        match timeout {
            Some(timeout) => tokio::time::timeout(timeout, receiver)
                .await
                .map_err(|_| ApiError::ConfigError("Timeout waiting for generation".to_string()))?
                .map_err(|_| ApiError::ConfigError("Completion channel closed".to_string()))?,
            None => receiver
                .await
                .map_err(|_| ApiError::ConfigError("Completion channel closed".to_string()))?,
        }
    }

    /// Worker loop for processing requests
    async fn worker_loop(
        worker_id: usize,
        queue: Arc<Mutex<BinaryHeap<GenerationRequest>>>,
        notify: Arc<Notify>,
        api: Arc<ContextApi>,
        config: GenerationConfig,
        rate_limiters: Arc<RwLock<HashMap<String, AgentRateLimiter>>>,
        running: Arc<RwLock<bool>>,
        stats: Arc<RwLock<QueueStats>>,
        event_context: Option<QueueEventContext>,
        dedupe_index: Arc<Mutex<HashMap<RequestIdentity, DedupeEntry>>>,
        metadata_builder: Arc<GeneratedMetadataBuilder>,
    ) {
        debug!(worker_id, "Worker started");

        while *running.read() {
            // Get next request from queue (highest priority first)
            let request = {
                let mut queue_guard = queue.lock().await;
                queue_guard.pop()
            };

            let Some(mut request) = request else {
                // No requests, wait for notification or timeout
                // Use a timeout to periodically check if we should stop
                let notify_future = notify.notified();
                let timeout_future = sleep(Duration::from_millis(100));
                tokio::select! {
                    _ = notify_future => {
                        // New item available, continue loop
                        continue;
                    }
                    _ = timeout_future => {
                        // Timeout, check if we should continue
                        continue;
                    }
                }
            };

            // Update stats
            {
                let mut stats = stats.write();
                stats.pending = stats.pending.saturating_sub(1);
                stats.processing += 1;
            }
            Self::emit_queue_stats_event_static(stats.clone(), event_context.clone());
            Self::emit_queue_event_static(
                event_context.clone(),
                "request_processing",
                QueueEventData {
                    node_id: hex::encode(request.node_id),
                    agent_id: request.agent_id.clone(),
                    provider_name: request.provider_name.clone(),
                    frame_type: request.frame_type.clone(),
                    request_id: Some(request.request_id.as_u64()),
                    retry_count: Some(request.retry_count),
                    duration_ms: None,
                },
            );

            // Get or create rate limiter for this agent
            // We need to clone the Arc references, not the limiter itself
            let (semaphore, last_request, min_delay) = {
                let mut limiters = rate_limiters.write();
                let limiter = limiters.entry(request.agent_id.clone()).or_insert_with(|| {
                    AgentRateLimiter::new(config.max_concurrent_per_agent, config.rate_limit_ms)
                });
                (
                    Arc::clone(&limiter.semaphore),
                    Arc::clone(&limiter.last_request),
                    limiter.min_delay,
                )
            };

            // Create a temporary rate limiter for this request
            let rate_limiter = AgentRateLimiter {
                semaphore,
                last_request,
                min_delay,
            };
            let request_identity = RequestIdentity::from_request(&request);

            // Acquire rate limiter permit
            let _permit = match rate_limiter.acquire(&request.agent_id).await {
                Ok(permit) => permit,
                Err(e) => {
                    error!(
                        worker_id,
                        agent_id = %request.agent_id,
                        error = %e,
                        "Failed to acquire rate limiter permit"
                    );
                    // Re-queue request (maintains priority order automatically)
                    let mut queue_guard = queue.lock().await;
                    queue_guard.push(request.clone());
                    {
                        let mut stats = stats.write();
                        stats.processing = stats.processing.saturating_sub(1);
                        stats.pending += 1;
                    }
                    continue;
                }
            };

            // Process request
            let result = Self::process_request(
                &request,
                &api,
                &config,
                event_context.clone(),
                metadata_builder.as_ref(),
            )
            .await;

            // Determine if we should retry (before sending result to completion channel)
            let should_retry = {
                let mut stats_guard = stats.write();
                stats_guard.processing = stats_guard.processing.saturating_sub(1);
                match &result {
                    Ok(_) => {
                        stats_guard.completed += 1;
                        false
                    }
                    Err(err) => {
                        // Check if we should retry
                        let retry = request.retry_count < config.max_retry_attempts
                            && Self::is_retryable_error(err);
                        if retry {
                            // Will update stats after re-queuing
                        } else {
                            stats_guard.failed += 1;
                            error!(
                                worker_id,
                                node_id = %hex::encode(request.node_id),
                                agent_id = %request.agent_id,
                                retry_count = request.retry_count,
                                error = %err,
                                "Generation request failed permanently"
                            );
                        }
                        retry
                    }
                }
            };
            Self::emit_queue_stats_event_static(stats.clone(), event_context.clone());

            if !should_retry {
                let waiters = {
                    let mut dedupe = dedupe_index.lock().await;
                    dedupe
                        .remove(&request_identity)
                        .map(|entry| entry.waiters)
                        .unwrap_or_default()
                };

                for tx in waiters {
                    let _ = tx.send(result.clone());
                }
            }

            // Re-queue if needed (after dropping stats guard)
            if should_retry {
                Self::emit_provider_event_static(
                    event_context.clone(),
                    "provider_request_retrying",
                    ProviderLifecycleEventData {
                        node_id: hex::encode(request.node_id),
                        agent_id: request.agent_id.clone(),
                        provider_name: request.provider_name.clone(),
                        frame_type: request.frame_type.clone(),
                        duration_ms: None,
                        error: None,
                        retry_count: Some(request.retry_count + 1),
                    },
                );
                request.retry_count += 1;
                // Add retry delay before re-queuing
                sleep(Duration::from_millis(config.retry_delay_ms)).await;

                let mut queue_guard = queue.lock().await;
                queue_guard.push(request.clone());
                drop(queue_guard);

                // Notify workers that a retry is available
                notify.notify_one();

                // Update stats after re-queuing
                let mut stats_guard = stats.write();
                stats_guard.pending += 1;
                drop(stats_guard);
                Self::emit_queue_stats_event_static(stats.clone(), event_context.clone());
            }
        }

        debug!(worker_id, "Worker stopped");
    }

    /// Process a single generation request
    /// This is the ONLY place where providers are called
    async fn process_request(
        request: &GenerationRequest,
        api: &ContextApi,
        _config: &GenerationConfig,
        event_context: Option<QueueEventContext>,
        metadata_builder: &GeneratedMetadataBuilder,
    ) -> Result<FrameID, ApiError> {
        debug!(
            request_id = ?request.request_id,
            node_id = %hex::encode(request.node_id),
            agent_id = %request.agent_id,
            attempt = request.retry_count + 1,
            "Processing generation request"
        );

        if !request.options.force {
            if let Some(existing_head) = api.get_head(&request.node_id, &request.frame_type)? {
                return Ok(existing_head);
            }
        }

        // Get agent
        let agent = api.get_agent(&request.agent_id)?;

        // Get provider config and type from registry (drop guard before await)
        let (provider_config, provider_type_str) = {
            let provider_registry = api.provider_registry().read();
            let config = provider_registry.get_or_error(&request.provider_name)?;
            let provider_type_str =
                crate::provider::profile::provider_type_slug(config.provider_type);
            (config.clone(), provider_type_str)
        };

        // Get node record
        let node_record = api
            .node_store()
            .get(&request.node_id)
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NodeNotFound(request.node_id))?;

        // Resolve agent prompt contract once through the explicit adapter.
        let prompt_contract = PromptContract::from_agent(&agent)?;
        let (system_prompt, user_prompt) = Self::generate_prompts(&prompt_contract, &node_record);

        // Create provider client (need to get registry again, but drop before await)
        let client = {
            let provider_registry = api.provider_registry().read();
            provider_registry.create_client(&request.provider_name)?
        };

        // Build and validate metadata through the shared write contract before provider IO.
        let generated_metadata = metadata_builder(
            &request.agent_id,
            &request.provider_name,
            client.model_name(),
            &provider_type_str,
            &user_prompt,
        );
        validate_frame_metadata(&generated_metadata, &request.agent_id)?;

        // Build prompt context based on node kind.
        // File nodes are grounded on live file bytes, while directory nodes
        // are grounded on child context frames.
        let prompt_context = match node_record.node_type {
            crate::store::NodeType::File { .. } => {
                Some(Self::collect_file_source_context(&node_record)?)
            }
            crate::store::NodeType::Directory => {
                let child_context_text =
                    Self::collect_directory_child_context_text(api, &node_record, request)?;
                if child_context_text.is_empty() {
                    let node_context_text = Self::collect_scoped_node_frame_context(api, request)?;
                    if node_context_text.is_empty() {
                        None
                    } else {
                        Some(node_context_text)
                    }
                } else {
                    Some(child_context_text)
                }
            }
        };

        // Build messages for LLM
        let mut messages = vec![ChatMessage {
            role: crate::provider::MessageRole::System,
            content: system_prompt,
        }];

        // Add context from existing frames
        if let Some(context_text) = prompt_context {
            messages.push(ChatMessage {
                role: crate::provider::MessageRole::User,
                content: format!("Context:\n{}\n\nTask: {}", context_text, user_prompt),
            });
        } else {
            messages.push(ChatMessage {
                role: crate::provider::MessageRole::User,
                content: user_prompt.clone(),
            });
        }

        // Resolve completion options: provider defaults > agent preferences (if any)
        let completion_options = provider_config.default_options.clone();

        // Agent preferences from metadata (optional hints, not requirements)
        // For now, we just use provider defaults. Agent preferences can be added later if needed.

        // Generate completion - THIS IS THE ONLY PLACE PROVIDERS ARE CALLED
        let start = Instant::now();
        info!(
            request_id = ?request.request_id,
            node_id = %hex::encode(request.node_id),
            agent_id = %request.agent_id,
            provider_name = %request.provider_name,
            frame_type = %request.frame_type,
            attempt = request.retry_count + 1,
            message_count = messages.len(),
            "Provider request sent"
        );
        Self::emit_provider_event_static(
            event_context.clone(),
            "provider_request_sent",
            ProviderLifecycleEventData {
                node_id: hex::encode(request.node_id),
                agent_id: request.agent_id.clone(),
                provider_name: request.provider_name.clone(),
                frame_type: request.frame_type.clone(),
                duration_ms: None,
                error: None,
                retry_count: Some(request.retry_count),
            },
        );
        let response = match client.complete(messages, completion_options).await {
            Ok(r) => Ok(r),
            Err(e) => {
                Self::emit_provider_event_static(
                    event_context.clone(),
                    "provider_request_failed",
                    ProviderLifecycleEventData {
                        node_id: hex::encode(request.node_id),
                        agent_id: request.agent_id.clone(),
                        provider_name: request.provider_name.clone(),
                        frame_type: request.frame_type.clone(),
                        duration_ms: Some(start.elapsed().as_millis()),
                        error: Some(e.to_string()),
                        retry_count: Some(request.retry_count),
                    },
                );
                // Enhance error with available models if model not found
                if let ApiError::ProviderModelNotFound(_) = e {
                    match client.list_models().await {
                        Ok(available_models) => {
                            if available_models.is_empty() {
                                Err(ApiError::ProviderModelNotFound(format!(
                                    "Model '{}' not found. Unable to retrieve available models list.",
                                    client.model_name()
                                )))
                            } else {
                                Err(ApiError::ProviderModelNotFound(format!(
                                    "Model '{}' not found. Available models: {}",
                                    client.model_name(),
                                    available_models.join(", ")
                                )))
                            }
                        }
                        Err(_) => Err(e),
                    }
                } else {
                    Err(e)
                }
            }
        }?;

        let duration = start.elapsed();
        info!(
            request_id = ?request.request_id,
            node_id = %hex::encode(request.node_id),
            agent_id = %request.agent_id,
            provider_name = %request.provider_name,
            frame_type = %request.frame_type,
            attempt = request.retry_count + 1,
            duration_ms = duration.as_millis(),
            response_chars = response.content.chars().count(),
            "Provider response received"
        );
        Self::emit_provider_event_static(
            event_context,
            "provider_response_received",
            ProviderLifecycleEventData {
                node_id: hex::encode(request.node_id),
                agent_id: request.agent_id.clone(),
                provider_name: request.provider_name.clone(),
                frame_type: request.frame_type.clone(),
                duration_ms: Some(duration.as_millis()),
                error: None,
                retry_count: Some(request.retry_count),
            },
        );

        // Create frame with generated content
        let basis = Basis::Node(request.node_id);
        let content = response.content.into_bytes();

        let frame = Frame::new(
            basis,
            content,
            request.frame_type.clone(),
            request.agent_id.clone(),
            generated_metadata,
        )?;

        // Store frame using put_frame
        let frame_id = api.put_frame(request.node_id, frame, request.agent_id.clone())?;

        info!(
            request_id = ?request.request_id,
            node_id = %hex::encode(request.node_id),
            agent_id = %request.agent_id,
            frame_id = %hex::encode(frame_id),
            duration_ms = duration.as_millis(),
            "Frame generation completed"
        );

        Ok(frame_id)
    }

    fn collect_directory_child_context_text(
        api: &ContextApi,
        node_record: &NodeRecord,
        request: &GenerationRequest,
    ) -> Result<String, ApiError> {
        if !matches!(node_record.node_type, crate::store::NodeType::Directory) {
            return Ok(String::new());
        }

        let child_view = ContextView::builder()
            .max_frames(1)
            .recent()
            .by_type(request.frame_type.clone())
            .by_agent(request.agent_id.clone())
            .build();

        let mut child_sections = Vec::new();
        for child_id in &node_record.children {
            let child_context = api.get_node(*child_id, child_view.clone())?;
            if child_context.frames.is_empty() {
                continue;
            }

            let child_kind = match child_context.node_record.node_type {
                crate::store::NodeType::File { .. } => "File",
                crate::store::NodeType::Directory => "Directory",
            };
            let child_text = child_context
                .frames
                .iter()
                .map(|f| String::from_utf8_lossy(&f.content))
                .collect::<Vec<_>>()
                .join("\n\n");

            child_sections.push(format!(
                "Path: {}\nType: {}\nContent:\n{}",
                child_context.node_record.path.display(),
                child_kind,
                child_text
            ));
        }

        if !child_sections.is_empty() {
            debug!(
                node_id = %hex::encode(node_record.node_id),
                direct_children = node_record.children.len(),
                child_context_nodes = child_sections.len(),
                frame_type = %request.frame_type,
                agent_id = %request.agent_id,
                "Collected directory child context for generation"
            );
        }

        Ok(child_sections.join("\n\n---\n\n"))
    }

    fn collect_scoped_node_frame_context(
        api: &ContextApi,
        request: &GenerationRequest,
    ) -> Result<String, ApiError> {
        let view = ContextView::builder()
            .max_frames(10)
            .recent()
            .by_type(request.frame_type.clone())
            .by_agent(request.agent_id.clone())
            .build();
        let context = api.get_node(request.node_id, view)?;
        Ok(context
            .frames
            .iter()
            .map(|f| String::from_utf8_lossy(&f.content))
            .collect::<Vec<_>>()
            .join("\n\n"))
    }

    fn collect_file_source_context(node_record: &NodeRecord) -> Result<String, ApiError> {
        if !matches!(node_record.node_type, crate::store::NodeType::File { .. }) {
            return Ok(String::new());
        }

        let bytes = std::fs::read(&node_record.path).map_err(|e| {
            ApiError::StorageError(crate::error::StorageError::IoError(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to read file source content for generation {}: {}",
                    node_record.path.display(),
                    e
                ),
            )))
        })?;

        let truncated = bytes.len() > Self::FILE_CONTEXT_MAX_BYTES;
        let slice = if truncated {
            &bytes[..Self::FILE_CONTEXT_MAX_BYTES]
        } else {
            &bytes
        };
        let mut text = String::from_utf8_lossy(slice).to_string();
        if truncated {
            text.push_str(&format!(
                "\n\n[Truncated to {} bytes for prompt safety]",
                Self::FILE_CONTEXT_MAX_BYTES
            ));
        }

        Ok(format!(
            "Path: {}\nType: File\nContent:\n{}",
            node_record.path.display(),
            text
        ))
    }

    /// Generate prompts from the explicit prompt contract adapter.
    fn generate_prompts(prompt_contract: &PromptContract, node_record: &NodeRecord) -> (String, String) {
        let user_prompt = prompt_contract.render_user_prompt(
            node_record.node_type.clone(),
            &node_record.path.display().to_string(),
            match node_record.node_type {
                crate::store::NodeType::File { size, .. } => Some(size),
                crate::store::NodeType::Directory => None,
            },
        );

        (prompt_contract.system_prompt.clone(), user_prompt)
    }

    /// Check if an error is retryable
    fn is_retryable_error(error: &ApiError) -> bool {
        match error {
            ApiError::ConfigError(_) => false, // Don't retry config errors
            ApiError::MissingPromptContractField { .. } => false,
            ApiError::FrameMetadataPolicyViolation(_) => false,
            ApiError::ProviderNotConfigured(_) => false,
            ApiError::ProviderRateLimit(_) => true,
            ApiError::ProviderRequestFailed(_) => true,
            ApiError::ProviderError(_) => true,
            _ => true, // Retry other errors by default
        }
    }

    fn emit_queue_event(&self, event_type: &str, payload: QueueEventData) {
        Self::emit_queue_event_static(self.event_context.clone(), event_type, payload);
    }

    fn emit_queue_event_static(
        event_context: Option<QueueEventContext>,
        event_type: &str,
        payload: QueueEventData,
    ) {
        if let Some(ctx) = event_context {
            ctx.progress
                .emit_event_best_effort(&ctx.session_id, event_type, json!(payload));
        }
    }

    fn emit_provider_event_static(
        event_context: Option<QueueEventContext>,
        event_type: &str,
        payload: ProviderLifecycleEventData,
    ) {
        if let Some(ctx) = event_context {
            ctx.progress
                .emit_event_best_effort(&ctx.session_id, event_type, json!(payload));
        }
    }

    fn emit_queue_stats_event(&self) {
        Self::emit_queue_stats_event_static(self.stats.clone(), self.event_context.clone());
    }

    fn emit_queue_stats_event_static(
        stats: Arc<RwLock<QueueStats>>,
        event_context: Option<QueueEventContext>,
    ) {
        if let Some(ctx) = event_context {
            let snapshot = stats.read().clone();
            ctx.progress.emit_event_best_effort(
                &ctx.session_id,
                "queue_stats",
                json!(QueueStatsEventData {
                    pending: snapshot.pending,
                    processing: snapshot.processing,
                    completed: snapshot.completed,
                    failed: snapshot.failed,
                }),
            );
        }
    }
}
