# Context Get Command Specification

## Overview

This document specifies the implementation details for `merkle context get`, which retrieves and renders context frames for nodes.

## CLI Functionality

### Command Structure

The command follows the existing CLI pattern in `src/tooling/cli.rs`:

```rust
#[derive(Subcommand)]
pub enum ContextCommands {
    // ... other commands ...
    /// Retrieve context frames for a node
    Get {
        /// Target node by NodeID (hex string)
        #[arg(long, conflicts_with = "path")]
        node: Option<String>,
        
        /// Target node by workspace-relative or absolute path
        #[arg(long, conflicts_with = "node")]
        path: Option<PathBuf>,
        
        /// Filter by agent ID
        #[arg(long)]
        agent: Option<String>,
        
        /// Filter by frame type
        #[arg(long)]
        frame_type: Option<String>,
        
        /// Maximum frames to return
        #[arg(long, default_value = "10")]
        max_frames: usize,
        
        /// Ordering policy: recency or deterministic
        #[arg(long, default_value = "recency")]
        ordering: String,
        
        /// Concatenate frame contents with separator
        #[arg(long)]
        combine: bool,
        
        /// Separator used with --combine
        #[arg(long, default_value = "\n\n---\n\n")]
        separator: String,
        
        /// Output format: text or json
        #[arg(long, default_value = "text")]
        format: String,
        
        /// Include metadata fields in output
        #[arg(long)]
        include_metadata: bool,
        
        /// Include frames marked deleted (tombstones)
        #[arg(long)]
        include_deleted: bool,
    },
}
```

### Command Execution Flow

1. **Path/NodeID Resolution**:
   - Same as generate command (see `context_generate_command.md`)
   - Resolve to `NodeID` using path or parse node_id

2. **Build ContextView**:
   - Use `ContextView::builder()` pattern
   - Set `max_frames` from `--max-frames`
   - Set ordering: `OrderingPolicy::Recency` or `OrderingPolicy::Deterministic` based on `--ordering`
   - Add filters:
     - `FrameFilter::ByAgent(agent_id)` if `--agent` provided
     - `FrameFilter::ByType(frame_type)` if `--frame-type` provided
   - Add `FrameFilter::ExcludeDeleted` if `--include-deleted` not set

3. **Retrieve Context**:
   - Call `api.get_node(node_id, view)` to get `NodeContext`

4. **Render Output**:
   - **Text format** (default):
     - If `--combine`: Use `context.combined_text(separator)`
     - Otherwise: Iterate frames and display each with metadata if `--include-metadata`
   - **JSON format**:
     - Serialize `NodeContext` to JSON
     - Include/exclude metadata based on `--include-metadata`
     - Filter out deleted frames if `--include-deleted` not set

## Required Logical Guards

### 1. Mutually Exclusive Options
- `--node` and `--path` must be mutually exclusive
- Exactly one must be provided

### 2. Node Validation
- Node must exist in store
- Path must be canonicalized and within workspace root

### 3. Ordering Validation
- `--ordering` must be "recency" or "deterministic"
- Return error for invalid ordering value

### 4. Format Validation
- `--format` must be "text" or "json"
- Return error for invalid format value

### 5. Filter Logic
- If `--include-deleted` not set, exclude frames with `metadata.deleted == true`
- Apply agent and frame_type filters before ordering

### 6. Separator Usage
- `--separator` only used when `--combine` is set
- Warn if `--separator` provided without `--combine` (or ignore)

## Tests Required

### CLI Tests (`tests/integration/context_cli.rs`)

1. **Path/NodeID Resolution Tests**:
   - `test_get_with_valid_path()` - Success with path
   - `test_get_with_valid_node_id()` - Success with node ID
   - `test_get_with_invalid_path()` - Path not in tree error
   - `test_get_with_nonexistent_node_id()` - NodeNotFound error

2. **Filter Tests**:
   - `test_get_filters_by_agent()` - Only returns frames for specified agent
   - `test_get_filters_by_frame_type()` - Only returns frames of specified type
   - `test_get_filters_by_agent_and_type()` - Combined filters work
   - `test_get_with_no_filters_returns_all()` - No filters returns all frames

3. **Ordering Tests**:
   - `test_get_ordering_recency()` - Most recent frames first
   - `test_get_ordering_deterministic()` - Deterministic ordering
   - `test_get_invalid_ordering_error()` - Error for invalid ordering

4. **Max Frames Tests**:
   - `test_get_respects_max_frames()` - Limits returned frames
   - `test_get_max_frames_default()` - Default is 10
   - `test_get_max_frames_zero_returns_all()` - Or returns all if 0? (clarify behavior)

5. **Combine Tests**:
   - `test_get_combine_concatenates_frames()` - Combines with separator
   - `test_get_combine_custom_separator()` - Uses custom separator
   - `test_get_combine_default_separator()` - Uses default separator
   - `test_get_without_combine_returns_individual()` - Without combine, returns individual frames

6. **Format Tests**:
   - `test_get_format_text()` - Text format output
   - `test_get_format_json()` - JSON format output
   - `test_get_invalid_format_error()` - Error for invalid format

7. **Metadata Tests**:
   - `test_get_include_metadata_text()` - Metadata included in text output
   - `test_get_include_metadata_json()` - Metadata included in JSON output
   - `test_get_exclude_metadata_default()` - Metadata excluded by default

8. **Deleted Frames Tests**:
   - `test_get_excludes_deleted_by_default()` - Deleted frames excluded
   - `test_get_include_deleted_shows_tombstones()` - Include deleted shows tombstones
   - `test_get_deleted_frames_have_empty_content()` - Tombstones have empty content

9. **Edge Cases**:
   - `test_get_with_no_frames_returns_empty()` - Empty result when no frames
   - `test_get_with_single_frame()` - Single frame handling
   - `test_get_with_many_frames_respects_limit()` - Many frames limited correctly

### Functional Tests (`tests/integration/context_api.rs`)

1. **View Construction Tests**:
   - `test_context_view_builder_pattern()` - Builder pattern works
   - `test_context_view_filters_apply()` - Filters applied correctly
   - `test_context_view_ordering_works()` - Ordering policies work

2. **Frame Retrieval Tests**:
   - `test_get_node_returns_correct_frames()` - Correct frames returned
   - `test_get_node_respects_max_frames()` - Max frames limit respected
   - `test_get_node_applies_filters()` - Filters applied correctly

3. **Output Formatting Tests**:
   - `test_combined_text_formatting()` - Combined text format correct
   - `test_json_serialization()` - JSON serialization correct
   - `test_text_output_includes_metadata()` - Metadata in text output
   - `test_json_output_includes_metadata()` - Metadata in JSON output

4. **Tombstone Handling Tests**:
   - `test_get_excludes_tombstones_by_default()` - Tombstones excluded
   - `test_get_includes_tombstones_when_requested()` - Tombstones included when requested
   - `test_tombstone_metadata_correct()` - Tombstone metadata correct

### Unit Tests (`src/tooling/cli.rs` - module tests)

1. **Output Formatting Tests**:
   - `test_format_text_output()` - Text formatting logic
   - `test_format_json_output()` - JSON formatting logic
   - `test_combine_frames_text()` - Frame combination logic

2. **Filter Application Tests**:
   - `test_apply_agent_filter()` - Agent filter application
   - `test_apply_frame_type_filter()` - Frame type filter application
   - `test_apply_deleted_filter()` - Deleted frame filter

## Patterns to Follow

### CLI Structure

1. **Command Location**: Add to `src/tooling/cli.rs` in `ContextCommands` enum
2. **Option Parsing**: Use clap with defaults where appropriate
3. **Validation**: Validate enum-like options (ordering, format) in execution logic
4. **Error Handling**: Return `Result<String, ApiError>`

### Logic Location

1. **Path Resolution**: Reuse `resolve_path_to_node_id()` helper from generate command
2. **View Building**: Use `ContextView::builder()` pattern from `src/api.rs`
3. **Context Retrieval**: Use `api.get_node()` from `src/api.rs`
4. **Output Formatting**: Create helper functions in `src/tooling/cli.rs`:
   - `format_text_output(context, include_metadata, combine, separator)`
   - `format_json_output(context, include_metadata)`

### Components to Use

1. **ContextApi**: Main API (`src/api.rs`)
   - `get_node(node_id, view)` - Get node context with view

2. **ContextView**: View builder (`src/api.rs`)
   - `ContextView::builder()` - Fluent builder API
   - `max_frames()`, `recent()`, `by_agent()`, `by_type()` methods

3. **NodeContext**: Response type (`src/api.rs`)
   - `frames` - Vector of frames
   - `combined_text(separator)` - Convenience method
   - `text_contents()` - Get all text contents

4. **OrderingPolicy**: Ordering enum (`src/views.rs`)
   - `Recency` - Most recent first
   - `Deterministic` - Deterministic ordering

5. **FrameFilter**: Filter enum (`src/views.rs`)
   - `ByAgent(agent_id)` - Filter by agent
   - `ByType(frame_type)` - Filter by type
   - `ExcludeDeleted` - Exclude deleted frames

6. **Frame**: Frame type (`src/frame/mod.rs`)
   - `text_content()` - Get text content
   - `metadata` - Access metadata
   - `is_type()` - Check frame type

### Output Format

**Text Format (default)**:
- Without `--combine`: One frame per section with headers
- With `--combine`: Single concatenated text with separator
- With `--include-metadata`: Include metadata annotations

**JSON Format**:
```json
{
  "node_id": "<hex>",
  "node_record": { ... },
  "frames": [
    {
      "frame_id": "<hex>",
      "frame_type": "...",
      "content": "...",
      "metadata": { ... }
    }
  ],
  "frame_count": 5
}
```

### Error Messages

- Use existing `ApiError` variants
- Include helpful context in error messages
- Suggest remediation when appropriate

### Logging

Use tracing macros:
- `info!()` for successful retrievals
- `debug!()` for detailed view construction
- `warn!()` for edge cases (e.g., no frames found)

