# Log File Design

## Summary

Meld already has a logging domain and file output support, yet default runtime behavior disables logging unless `--verbose` is passed.
This design adds default on log to file with cross platform path resolution and keeps initial scope small.

## Goals

- Enable log emission by default
- Write logs to file by default
- Use a cross platform default log path
- Keep adapter code thin and place path policy in logging domain
- Keep operational risk low with focused tests

## Non Goals

- Log rotation in initial release
- Full backward compatibility with old logging behavior
- New user command for explicit flush in initial release

## Current Gaps

- CLI forces `level = off` when `--verbose` is not set
- Output parser accepts `stderr` and `both` but runtime does not route those paths
- Default path logic relies on Unix style environment assumptions
- Spec mentions `MERKLE_LOG_FILE` but runtime does not apply it
- Rotation fields exist but are not wired

## Proposed Behavior

### Default Behavior

- Logging enabled by default
- Default level remains `info`
- Default output becomes `file`
- Default format remains `text`
- Default file path is resolved at runtime from platform state directory policy

### Precedence

Highest to lowest precedence

1. CLI flags
2. Environment variables
3. Config file
4. Runtime defaults

### Output Modes

- `file`
- `stdout`
- `stderr`
- `file+stderr`

## Path Resolution

Use `directories::ProjectDirs` for platform aware base directories.

Resolution order

1. CLI `--log-file`
2. Env `MERKLE_LOG_FILE`
3. Config `logging.file`
4. Default derived from `ProjectDirs` state location and workspace scoped path segment

Workspace scoping should reuse existing canonical workspace path strategy already used in data path helpers.

## Config Shape

Keep config in `[logging]`.

- `enabled` bool default `true`
- `level` string default `info`
- `format` string default `text`
- `output` string default `file`
- `file` optional path
- `color` bool default `true`
- `modules` map

Remove unused rotation keys now to avoid dead config

- `rotation`
- `max_file_size`
- `max_files`

## Runtime Architecture

### Logging Domain

- Add path resolver function in `src/logging.rs`
- Add env parsing for `MERKLE_LOG_FILE`
- Add output parser support for `file+stderr`
- Compose subscriber layers for file and stderr when selected
- Use non blocking file writer and hold guard for process lifetime

### Tooling Adapter

- Update `src/bin/meld.rs`
- Stop forcing log off when `--verbose` is absent
- Reinterpret `--verbose` as level override to `debug`
- Add `--quiet` to set `enabled = false`

## Flush Strategy

Initial strategy

- Rely on non blocking guard drop at process exit
- No dedicated CLI flush command in initial release

Future option

- Add internal flush hook in logging domain if watch runtime needs explicit sync point

## Tests

Add focused coverage

- Unit tests for output parsing including `file+stderr`
- Unit tests for path resolution precedence
- Unit tests for default path fallback when no explicit file is set
- Integration test that emits one log line and verifies file write
- CLI behavior test for default logging without `--verbose`

## Migration Notes

- Existing users who relied on silent default runs will now get file logs by default
- Users can disable via config `logging.enabled = false` or CLI `--quiet`
- No compatibility layer is planned

## Rollout Plan

1. Implement config and runtime behavior changes
2. Update CLI flags and wiring
3. Add tests for path and sink behavior
4. Update README and config examples
5. Validate with targeted test runs
