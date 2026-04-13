# Repo context

Rust CLI to list and delete local session history for Claude Code, Codex, and Droid.

## Where to look

- `src/lib.rs`: command orchestration for `list`, `select`, and `nuke`, plus tty gating and top-level delete flow
- `src/interactive/mod.rs`: dialoguer prompts and selected-session deletion wiring
- `src/interactive/tui/`: ratatui browser, app state, rendering, and terminal setup for interactive selection
- `src/sources/mod.rs`: source registry, root/env helpers, recursive jsonl collection, and guarded deletion utilities
- `src/sources/claude_code.rs`: Claude Code discovery for both jsonl files and directory-backed sessions, plus empty-dir pruning on delete
- `src/sources/codex.rs`: Codex session discovery from `session_meta` records in nested jsonl files
- `src/sources/droid.rs`: Droid session discovery from `session_start` records and paired `.settings.json` deletion
- `src/model/session.rs`: domain types plus grouped session rendering helpers
- `src/ui/cli.rs`: clap command and flag definitions
- `src/ui/output.rs`: human-readable CLI output
- `tests/cli.rs`: integration tests and temp-root fixture helpers

## Command behavior

- `select` is the default command and opens the interactive picker
- `nuke` only works with `--all`
- `nuke --all` also requires `--tool` when no interactive terminal is attached

## Workflow

Run in order, fix if not green before done:

```sh
cargo fmt
cargo test
cargo clippy
```

## Location mapping

Session data paths (overridable via env):

| Tool        | Default path           | Env override                    |
|-------------|------------------------|---------------------------------|
| Claude Code | `~/.claude/projects`   | `NUKE_MY_SESSIONS_CLAUDE_ROOT`  |
| Codex       | `~/.codex/sessions`    | `NUKE_MY_SESSIONS_CODEX_ROOT`   |
| Droid       | `~/.factory/sessions`  | `NUKE_MY_SESSIONS_DROID_ROOT`   |
