# Logging Specification

## Overview

This specification defines the logging system for the Merkle filesystem state management system. The logging system provides structured, leveled logging to enable observability, debugging, and monitoring of system operations.

## Goals

### Primary Goals
- **Observability**: Provide insight into system operations and state
- **Debugging**: Enable efficient troubleshooting of issues
- **Performance Monitoring**: Track operation durations and resource usage
- **Structured Data**: Log in a format suitable for parsing and analysis
- **Configurable**: Allow runtime configuration of log levels and outputs

### Secondary Goals
- **Low Overhead**: Minimize performance impact of logging
- **Context Preservation**: Maintain request/operation context through logs
- **Production Ready**: Suitable for both development and production use

## Log Levels

### Level Hierarchy

The system uses standard log levels in order of verbosity:

1. **TRACE** - Most verbose, detailed execution flow
2. **DEBUG** - Detailed diagnostic information
3. **INFO** - General informational messages
4. **WARN** - Warning messages for recoverable issues
5. **ERROR** - Error messages for failures
6. **OFF** - No logging

### Level Usage Guidelines

#### TRACE
Use for extremely detailed execution flow, typically only needed when debugging specific issues.

**When to use:**
- Function entry/exit points
- Detailed state transitions
- Loop iterations (for small loops)
- Hash computations
- Lock acquisition/release

**Example:**
```rust
trace!("Entering hash_file: path={:?}", path);
trace!("Computed content hash: {}", hex::encode(hash));
trace!("Exiting hash_file: node_id={}", hex::encode(node_id));
```

#### DEBUG
Use for diagnostic information useful during development and troubleshooting.

**When to use:**
- Operation start/completion
- Key decision points
- State changes
- Configuration loading
- Cache hits/misses
- Event processing details

**Example:**
```rust
debug!("Processing {} change events", events.len());
debug!("Tree update: {} nodes affected", affected_nodes.len());
debug!("Regenerating frame: node_id={}, frame_type={}",
       hex::encode(node_id), frame_type);
```

#### INFO
Use for significant events that are useful in normal operation.

**When to use:**
- System initialization
- Workspace scans
- Tree updates
- Frame creation
- Regeneration completion
- Watch mode start/stop
- Configuration changes

**Example:**
```rust
info!("Scanned {} nodes (root: {})", node_count, hex::encode(root_id));
info!("Frame created: frame_id={}, node_id={}, agent={}",
      hex::encode(frame_id), hex::encode(node_id), agent_id);
info!("Watch mode started: workspace={:?}", workspace_root);
```

#### WARN
Use for recoverable issues or unexpected but handled conditions.

**When to use:**
- Retry attempts
- Fallback behavior
- Missing optional data
- Performance degradation
- Deprecated feature usage
- Configuration issues

**Example:**
```rust
warn!("Node not found in store, will rebuild: node_id={}", hex::encode(node_id));
warn!("Retry attempt {} failed, retrying: error={}", attempt, error);
warn!("High event queue size: {} events queued", queue_size);
```

#### ERROR
Use for failures that prevent normal operation or indicate bugs.

**When to use:**
- Operation failures
- Unrecoverable errors
- Data corruption
- System errors
- Provider failures

**Example:**
```rust
error!("Failed to build tree: error={}", error);
error!("Storage write failed: path={:?}, error={}", path, error);
error!("Provider request failed: provider={}, error={}", provider, error);
```

## Structured Logging

### Log Format

Logs use structured JSON format for machine parsing while remaining human-readable.

#### JSON Format
```json
{
  "timestamp": "2024-01-01T12:00:00.123456Z",
  "level": "info",
  "target": "merkle::tree::builder",
  "message": "Tree build completed",
  "fields": {
    "node_count": 157,
    "root_id": "1f1e75424cfaa4dc7c1df9ed4c9ecb083489da8fdfb91a00db687b49009f06e8",
    "duration_ms": 45
  }
}
```

#### Human-Readable Format (Default)
```
2024-01-01T12:00:00.123456Z INFO merkle::tree::builder: Tree build completed node_count=157 root_id=1f1e75424cfaa4dc7c1df9ed4c9ecb083489da8fdfb91a00db687b49009f06e8 duration_ms=45
```

### Standard Fields

All log entries include:

- **timestamp**: ISO 8601 timestamp with microsecond precision
- **level**: Log level (trace, debug, info, warn, error)
- **target**: Module path (e.g., `merkle::api::ContextApi`)
- **message**: Human-readable message

### Context Fields

Common context fields for different operation types:

#### File Operations
- `path`: File or directory path
- `node_id`: NodeID (hex encoded)
- `file_size`: File size in bytes
- `operation`: Operation type (read, write, delete)

#### Tree Operations
- `root_id`: Root NodeID (hex encoded)
- `node_count`: Number of nodes
- `depth`: Tree depth
- `operation`: Operation type (build, update, rebuild)

#### Frame Operations
- `frame_id`: FrameID (hex encoded)
- `node_id`: NodeID (hex encoded)
- `frame_type`: Frame type string
- `agent_id`: Agent identifier
- `operation`: Operation type (create, read, update, delete)

#### Watch Mode
- `event_count`: Number of events processed
- `event_type`: Type of filesystem event
- `path`: Affected path
- `duration_ms`: Processing duration
- `queue_size`: Event queue size

#### Regeneration
- `node_id`: NodeID being regenerated
- `frame_count`: Number of frames regenerated
- `duration_ms`: Regeneration duration
- `recursive`: Whether recursive regeneration

#### Provider Operations
- `provider`: Provider name
- `model`: Model name
- `request_id`: Request identifier
- `duration_ms`: Request duration
- `tokens`: Token usage

## Configuration

### Configuration File

Logging configuration in `config.toml`:

```toml
[logging]
# Log level: trace, debug, info, warn, error, off
level = "info"

# Output format: json, text (default: text)
format = "text"

# Output destination: stdout, stderr, file, both
output = "stdout"

# Log file path (if output includes "file")
file = ".merkle/merkle.log"

# Enable log rotation
rotation = true

# Maximum log file size before rotation (bytes)
max_file_size = 10485760  # 10 MB

# Number of rotated log files to keep
max_files = 5

# Enable colored output (text format only, stdout/stderr only)
color = true

# Module-specific log levels
[logging.modules]
# Override log level for specific modules
"merkle::tree" = "debug"
"merkle::watch" = "info"
"merkle::provider" = "warn"
```

### Environment Variables

Logging can also be configured via environment variables:

```bash
# Set log level
MERKLE_LOG=debug

# Set log format
MERKLE_LOG_FORMAT=json

# Set output
MERKLE_LOG_OUTPUT=file

# Set log file
MERKLE_LOG_FILE=/var/log/merkle.log

# Module-specific levels (comma-separated)
MERKLE_LOG_MODULES=merkle::tree=debug,merkle::watch=info
```

### CLI Arguments

Logging can be configured via CLI arguments:

```bash
# Set log level
merkle --log-level debug scan

# Set log format
merkle --log-format json watch

# Set log file
merkle --log-file /var/log/merkle.log watch
```

### Priority Order

Configuration priority (highest to lowest):
1. CLI arguments
2. Environment variables
3. Configuration file
4. Defaults

## Implementation

### Logging Library

Use `tracing` crate for structured logging:

```toml
[dependencies]
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json", "ansi"] }
```

### Initialization

Initialize logging in `main()`:

```rust
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};
use tracing_subscriber::fmt;

fn init_logging(config: &LoggingConfig) -> Result<(), Error> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.level));

    let subscriber = Registry::default()
        .with(filter)
        .with(fmt::layer()
            .with_target(true)
            .with_timer(fmt::time::ChronoUtc::rfc_3339())
            .with_ansi(config.color && config.output != "file")
        );

    subscriber.init();

    Ok(())
}
```

### Usage Examples

#### Basic Logging
```rust
use tracing::{debug, info, warn, error};

info!("Workspace scan started");
debug!("Processing file: path={:?}", path);
warn!("High memory usage: {} MB", memory_mb);
error!("Operation failed: error={}", error);
```

#### Structured Logging
```rust
use tracing::{info, instrument};

#[instrument(skip(self), fields(node_id = %hex::encode(node_id)))]
fn get_node(&self, node_id: NodeID) -> Result<NodeContext, ApiError> {
    info!("Retrieving node context");
    // ... implementation
}

// Or with explicit fields
info!(
    node_id = %hex::encode(node_id),
    frame_count = frames.len(),
    "Node context retrieved"
);
```

#### Span-based Context
```rust
use tracing::{info_span, Instrument};

async fn process_events(events: Vec<Event>) -> Result<(), Error> {
    let span = info_span!("process_events", event_count = events.len());
    let _enter = span.enter();

    info!("Starting event processing");
    // ... processing
    info!("Event processing completed");

    Ok(())
}
```

## Logging Points

### Core Operations

#### Tree Building
- **INFO**: Scan started/completed, node count, root hash
- **DEBUG**: Individual file/directory processing
- **TRACE**: Hash computations, path canonicalization
- **WARN**: Skipped files (permissions, symlinks)
- **ERROR**: Build failures, I/O errors

#### Node Operations
- **INFO**: Node creation, updates
- **DEBUG**: Node lookups, cache hits/misses
- **TRACE**: NodeID computations
- **WARN**: Node not found (recoverable)
- **ERROR**: Storage errors, corruption

#### Frame Operations
- **INFO**: Frame creation, updates, deletions
- **DEBUG**: Frame lookups, basis computations
- **TRACE**: FrameID computations, content hashing
- **WARN**: Frame not found (recoverable)
- **ERROR**: Storage errors, invalid frames

#### Synthesis
- **INFO**: Synthesis started/completed
- **DEBUG**: Child frame collection, policy application
- **TRACE**: Content concatenation, summarization
- **WARN**: Missing child frames
- **ERROR**: Synthesis failures

#### Regeneration
- **INFO**: Regeneration started/completed, frame count
- **DEBUG**: Basis change detection, affected frames
- **TRACE**: Basis hash comparisons
- **WARN**: Regeneration skipped (no changes)
- **ERROR**: Regeneration failures

### Watch Mode

#### Event Processing
- **INFO**: Event batch processed, nodes updated
- **DEBUG**: Individual event processing, batching
- **TRACE**: Event queue operations, debouncing
- **WARN**: High queue size, slow processing
- **ERROR**: Event processing failures

#### Tree Updates
- **INFO**: Tree update completed, affected nodes
- **DEBUG**: Incremental update details
- **TRACE**: NodeID recomputations
- **WARN**: Full rebuild required
- **ERROR**: Update failures

### Provider Operations

#### API Requests
- **INFO**: Request sent, response received
- **DEBUG**: Request/response details (sanitized)
- **TRACE**: HTTP details, retries
- **WARN**: Rate limiting, retries
- **ERROR**: Request failures, authentication errors

## Performance Considerations

### Logging Overhead

- **TRACE**: Can significantly impact performance, disable in production
- **DEBUG**: Moderate overhead, useful for troubleshooting
- **INFO**: Low overhead, safe for production
- **WARN/ERROR**: Minimal overhead, always enabled

### Optimization Strategies

1. **Conditional Compilation**: Use `#[cfg(debug_assertions)]` for expensive trace logs
2. **Lazy Evaluation**: Use closures for expensive log formatting
3. **Sampling**: Sample high-frequency logs (e.g., every Nth event)
4. **Async Logging**: Use async logging to avoid blocking operations
5. **Filtering**: Filter logs at the source for high-frequency operations

### Example: Lazy Evaluation
```rust
// Expensive operation only executed if DEBUG level is enabled
debug!(target: "merkle::tree", "Tree structure: {}",
       || format!("{:?}", expensive_tree_format()));
```

## Log Rotation

### File Rotation

When logging to file, implement log rotation:

- **Size-based**: Rotate when file exceeds max size
- **Time-based**: Rotate daily at midnight
- **Naming**: `merkle.log`, `merkle.log.1`, `merkle.log.2`, etc.
- **Compression**: Compress old log files (optional)

### Configuration
```toml
[logging]
rotation = true
max_file_size = 10485760  # 10 MB
max_files = 5
compress_old = true
```

## Security Considerations

### Sensitive Data

Never log sensitive information:

- **API Keys**: Never log API keys or tokens
- **Passwords**: Never log passwords
- **Personal Data**: Be careful with user data
- **File Contents**: Don't log file contents (only metadata)

### Sanitization

Sanitize data before logging:

```rust
// Sanitize API keys
let sanitized_key = if api_key.len() > 8 {
    format!("{}...{}", &api_key[..4], &api_key[api_key.len()-4..])
} else {
    "***".to_string()
};
debug!("Provider API key: {}", sanitized_key);
```

## Testing

### Test Logging

In tests, use a test logger that captures logs:

```rust
#[cfg(test)]
mod tests {
    use tracing_test::traced_test;

    #[traced_test]
    #[test]
    fn test_operation() {
        // Logs are captured and can be asserted
        info!("Test operation");
        // ... test code
    }
}
```

### Log Assertions

Assert log output in tests:

```rust
#[test]
fn test_logging() {
    let _guard = tracing_subscriber::fmt()
        .with_test_writer()
        .set_default();

    info!("Test message");
    // Assert log output
}
```

## Integration Points

### Error Handling

Integrate logging with error handling:

```rust
match operation() {
    Ok(result) => {
        info!("Operation succeeded: result={:?}", result);
        result
    }
    Err(e) => {
        error!("Operation failed: error={}", e);
        Err(e)
    }
}
```

### Metrics

Log metrics for monitoring:

```rust
let start = Instant::now();
// ... operation
let duration = start.elapsed();
info!(
    operation = "get_node",
    duration_ms = duration.as_millis(),
    "Operation completed"
);
```

## Migration Plan

### Phase 1: Add Logging Infrastructure
1. Add `tracing` and `tracing-subscriber` dependencies
2. Create logging configuration structure
3. Initialize logging in `main()`
4. Add basic logging to key operations

### Phase 2: Replace Existing Output
1. Replace `eprintln!` with appropriate log levels
2. Replace `println!` with `info!` or `debug!`
3. Add structured fields to log messages
4. Update CLI to support log configuration

### Phase 3: Comprehensive Logging
1. Add logging to all major operations
2. Add span-based context tracking
3. Implement log rotation
4. Add performance logging

### Phase 4: Production Hardening
1. Optimize logging performance
2. Add log sampling for high-frequency operations
3. Implement log sanitization
4. Add monitoring integration

## Examples

### Complete Example: Tree Building

```rust
use tracing::{info, debug, warn, error, instrument};

#[instrument(skip(self), fields(workspace = %self.root.display()))]
pub fn build(&self) -> Result<Tree, StorageError> {
    info!("Starting tree build");

    let start = Instant::now();
    let walker = Walker::new(self.root.clone());

    let entries = match walker.walk() {
        Ok(e) => {
            debug!("Walked filesystem: {} entries", e.len());
            e
        }
        Err(e) => {
            error!("Filesystem walk failed: error={}", e);
            return Err(e);
        }
    };

    // ... build tree

    let duration = start.elapsed();
    info!(
        node_count = tree.nodes.len(),
        root_id = %hex::encode(tree.root_id),
        duration_ms = duration.as_millis(),
        "Tree build completed"
    );

    Ok(tree)
}
```

## References

- [tracing documentation](https://docs.rs/tracing/)
- [tracing-subscriber documentation](https://docs.rs/tracing-subscriber/)
- [Watch Mode Specification](watch_mode_spec.md) - Observability section
- [Phase 2 Specification](phase2_spec.md) - Error handling section
