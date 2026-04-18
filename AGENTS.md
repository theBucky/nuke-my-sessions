# Repo Context

Rust CLI to list and delete local session history for Claude Code, Codex, and Droid.

## Where to Look

- `src/main.rs`: thin binary entrypoint into `nuke_my_sessions::run`
- `src/lib.rs`: command orchestration for `list`, `select`, and `nuke`, plus tty gating and top-level delete flow
- `src/interactive/mod.rs`: dialoguer prompts and selected-session deletion wiring
- `src/interactive/tui/mod.rs`: interactive browser loop, shared tui types, and terminal event handling
- `src/interactive/tui/app.rs`: browser state transitions, row-cache derivation, selection logic, and delete refresh flow
- `src/interactive/tui/render.rs`: ratatui rendering for tool list, session list, and footer state
- `src/interactive/tui/terminal.rs`: terminal enter and restore guard
- `src/sources/mod.rs`: source registry, root/env helpers, jsonl folds, recursive file discovery, and guarded deletion utilities
- `src/sources/claude_code.rs`: Claude Code discovery for both jsonl files and directory-backed sessions, plus empty-dir pruning on delete
- `src/sources/codex.rs`: Codex session discovery from `session_meta` records in nested jsonl files
- `src/sources/droid.rs`: Droid session discovery from `session_start` records and paired `.settings.json` deletion
- `src/model/session.rs`: domain types plus shared grouped-session derivation for cli and tui rendering
- `src/ui/cli.rs`: clap subcommands and global flag definitions
- `src/ui/output.rs`: human-readable CLI output
- `tests/cli.rs`: integration tests and temp-root fixture helpers

## Repo Style

- prefer functional programming for pure transformations
- derive values with `map`, `filter`, `fold`, `chunk_by`, and iterator composition
- keep mutation at real boundaries such as filesystem effects, terminal io, and tui state transitions
- share derived data shapes across cli and tui code
- add abstractions only when more than one runtime path consumes the same behavior or data shape

## Workflow

Run in order, fix if not green before done:

```sh
cargo fmt
cargo test
cargo clippy
```

## Location Mapping

Session data paths:

| Tool        | Default Path         |
|-------------|----------------------|
| Claude Code | `~/.claude/projects` |
| Codex       | `~/.codex/sessions`  |
| Droid       | `~/.factory/sessions` |
