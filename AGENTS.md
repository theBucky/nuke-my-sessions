# Repo context

Rust CLI to nuke all or selected sessions in Claude Code, Codex, and Droid.

## Where to look

- `src/lib.rs`: command orchestration for `list`, `select`, and `nuke`
- `src/delete_flow.rs`: dialoguer prompts, scoped selection, deletion wiring
- `src/delete_flow/tui.rs`: ratatui session browser and keyboard flow
- `src/sources/`: per-tool session discovery; `mod.rs` owns registry, shared root helpers, jsonl collection, and guarded deletion
- `src/model/session.rs`: domain types plus grouped session rendering helpers
- `src/ui/`: CLI args and output formatting
- `tests/cli.rs`: integration tests and temp-root fixture helpers

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
