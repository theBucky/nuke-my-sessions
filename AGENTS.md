# Repo context

Rust CLI to nuke all or selected sessions in Claude Code, Codex, and Droid.

## Where to look

- `src/sources/`: session discovery and deletion per tool
- `src/delete_flow/`: interactive selection UI (paging, rows)
- `src/model/`: domain types (`SessionEntry`, `Tool`)
- `src/ui/`: CLI args and output formatting
- `tests/cli.rs`: integration tests

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
