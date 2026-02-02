# Watch Command Specification

## Overview

This document specifies the watch command that runs the file-watcher daemon so the Merkle tree (and optionally context) stays updated on filesystem changes. Watch uses the same ignore sources as scan: built-in defaults, .gitignore, and the per-workspace ignore list. There is no CLI `--ignore` flag; ignore behavior is defined by [ignore_list_spec.md](ignore_list_spec.md). For full workflow and architecture (File Watcher, Event Queue, Batcher, Change Processor, Tree Builder, Node Updater, Regenerator), see design/workflow/watch_mode_spec.md.

## Command structure

**Syntax**

```
merkle watch [OPTIONS]
```

**Options**

- `--workspace <PATH>`: Workspace root directory (default: current directory).
- `--config <PATH>`: Configuration file path.
- `--debounce-ms <MS>`: Debounce window in milliseconds (default: 100).
- `--batch-window-ms <MS>`: Batch window in milliseconds (default: 50 or per config).
- `--recursive`: Enable recursive regeneration (regenerate parent frames when basis changes).
- `--max-depth <N>`: Maximum regeneration depth (default: 3).
- `--agent-id <ID>`: Agent ID for automatic regeneration (default from config, e.g. "watch-daemon").
- `--log-level <LEVEL>`: Log level (trace, debug, info, warn, error).
- `--log-file <PATH>`: Log file path (default: stdout).
- `--foreground`: Run in foreground (default: background daemon).
- `--pid-file <PATH>`: PID file path (default: .merkle/watch.pid).
- `--stop`: Stop the watch daemon (if running).

Exact option names and defaults should match the existing CLI and design/workflow/watch_mode_spec.md.

## Ignore behavior

Watch loads ignore patterns the same way as scan. At daemon start (and when processing events that affect the tree), resolve workspace root, then read built-in defaults, `workspace_root/.gitignore` (if present), and `workspace_data_dir(workspace_root).join("ignore_list")` (if present). Merge them into the Walker/TreeBuilder config. Paths matching these patterns are not included in the tree. See [ignore_list_spec.md](ignore_list_spec.md). There is no `--ignore` option; watch does not accept CLI override for ignore patterns.

## Execution flow

1. **Load configuration:** Resolve workspace root from CLI or config; load ignore list and .gitignore per ignore_list_spec; load agent and provider registries if needed for regeneration; initialize storage backends (node store, frame storage).
2. **Build initial tree (optional):** Optionally perform a full scan (using the same ignore sources) so the daemon starts with a populated tree; or rely on user having run scan previously.
3. **Initialize watcher:** Create file watcher for workspace root (recursive); set up event channel (e.g. notify crate).
4. **Create WatchDaemon:** Build WatchConfig from CLI options and config; instantiate WatchDaemon with API and config; tree updates use the merged ignore patterns.
5. **Start daemon:** Call daemon run loop; process events (debounce, batch), update tree via TreeBuilder/NodeRecordStore (respecting ignore patterns), optionally trigger regeneration for affected nodes.
6. **Shutdown:** On signal or `--stop`, shut down watcher and workers gracefully.

## Required guards

- **Workspace root:** Must exist and be readable; watcher must be scoped to workspace only.
- **Single instance:** Use PID file or similar to avoid multiple daemons for the same workspace (or document behavior).
- **Resource limits:** Bounded event queue and batch size to avoid memory exhaustion; see watch_mode_spec.md.

## Output

**Text (start):**

- Confirmation that watch has started (e.g. "Watch daemon started" or PID and path).

**Text (stop):**

- When using `--stop`: "Watch daemon stopped" or equivalent.

**Errors:**

- Watcher initialization failure, workspace not found, config errors; propagate with clear messages.

**JSON:** Not required for watch in this spec; logging may be structured per watch_mode_spec.md.

## Implementation

- **CLI:** `Commands::Watch { debounce_ms, batch_window_ms, … }` in `src/tooling/cli.rs`; dispatch to watch handler. No `--ignore` flag.
- **Daemon:** `WatchDaemon` and `WatchConfig` in `src/tooling/watch.rs`; file watcher (e.g. notify), event queue, batcher, change processor, tree builder, optional regenerator. Tree updates use the same ignore sources as scan (ignore_list + .gitignore + defaults).
- **Config:** Merge CLI flags with config file; see watch_mode_spec.md for [watch] section.

## Tests required

- Integration: Start watch in foreground with short timeout or mock watcher; verify process starts and accepts events (or exits cleanly).
- Integration: Verify debounce/batch options are passed to WatchConfig.
- Unit: WatchConfig construction from CLI and config; event batching logic if testable in isolation.
- Optional: Stop command finds PID file and signals daemon; daemon exits cleanly.
