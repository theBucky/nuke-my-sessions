# spec

## goal

build a simple Rust CLI that lists sessions from Claude Code and Codex, lets the user select all or some of them, confirms destructive actions, and deletes the chosen sessions safely.

## scope

- support Claude Code sessions
- support Codex sessions
- support deleting all sessions for a tool
- support deleting selected sessions for a tool
- support interactive selection in the terminal
- support non-interactive flags for scripted use

## non-goals

- no full-screen TUI app
- no database
- no network service
- no background daemon
- no config system unless real config complexity appears later

## stack

- Rust stable
- single Cargo package unless source adapters become large enough to justify a workspace later
- `clap` for command parsing and help output
- `dialoguer` for interactive selection and confirmation
- `serde` and `serde_json` for reading session metadata if stored as JSON
- `directories` for resolving tool state directories portably
- `anyhow` for error handling
- `tracing` and `tracing-subscriber` only for optional verbose logging
- `tempfile` for tests
- `assert_cmd` and `predicates` for CLI tests

## UX

### interactive behavior

- user can move up and down through session entries with arrow keys
- user can toggle session selection with `space`
- user submits selection with `enter`
- destructive actions always require explicit confirmation
- if no sessions exist, print a clear no-op message and exit successfully

### interface choice

- use `dialoguer::MultiSelect` for selecting multiple sessions
- use `dialoguer::Select` for single-choice menus when needed
- do not use `ratatui` in v1

reason:

- required interaction, up/down navigation plus submit, is already covered by `dialoguer`
- `ratatui` adds full-screen app complexity, more rendering state, and more testing overhead without solving a real v1 need

## CLI shape

```text
nuke-my-sessions [OPTIONS] [COMMAND]

commands:
  list
  select
  nuke

options:
  --tool <claude-code|codex>
  --all
  --yes
  --verbose
```

behavior:

- `list`: print discovered sessions
- `select`: open interactive selection flow, then confirm and delete selected sessions
- `nuke --all`: delete all sessions for the chosen tool after confirmation
- `--yes`: skip confirmation for scripted use only
- if command is omitted, default to interactive selection flow

## architecture

modules:

- `cli`: argument parsing and command dispatch
- `session`: shared session model
- `sources`: adapter trait and source implementations
- `sources::claude_code`: Claude Code session discovery and deletion
- `sources::codex`: Codex session discovery and deletion
- `delete_flow`: selection, confirmation, and delete orchestration
- `output`: human-readable terminal output

core types:

```rust
struct SessionEntry {
    tool: Tool,
    id: String,
    label: String,
    path: PathBuf,
    updated_at: Option<SystemTime>,
}

enum Tool {
    ClaudeCode,
    Codex,
}

trait SessionSource {
    fn list_sessions(&self) -> Result<Vec<SessionEntry>>;
    fn delete_sessions(&self, sessions: &[SessionEntry]) -> Result<()>;
}
```

## flow

1. resolve storage path for selected tool
2. discover sessions
3. normalize them into `SessionEntry`
4. present interactive or non-interactive selection
5. print summary of pending deletion
6. require confirmation unless `--yes`
7. delete selected session paths
8. print success summary and count

## safety rules

- never delete outside the resolved tool storage roots
- fail fast on invalid paths or missing metadata assumptions
- show exact session count before confirmation
- if one deletion fails, report it clearly and exit non-zero
- avoid partial silent success, summarize deleted and failed items explicitly

## implementation notes

- start with synchronous filesystem access
- add async only if profiling later shows real need
- keep control flow flat, use early returns
- keep logging off by default, enable only with `--verbose`

## test plan

- unit test session discovery against fixture directories
- unit test path filtering and safety checks
- integration test `list`
- integration test interactive-independent destructive flows using `--all --yes`
- integration test deleting selected mocked sessions through orchestrator layer
- never point tests at real user session directories
