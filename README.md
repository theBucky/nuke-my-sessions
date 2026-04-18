# nuke-my-sessions

Delete local session history for Claude Code, Codex, and Droid.

## Installation

```sh
cargo install --path .
```

## Usage

```sh
nuke-my-sessions              # interactive picker (default)
nuke-my-sessions list         # print sessions grouped by project
nuke-my-sessions nuke --all   # delete all sessions for a tool
```

### Commands

| Command | Description |
|---------|-------------|
| `select` | Interactive picker to choose sessions for deletion (default) |
| `list` | Print sessions grouped by project |
| `nuke --all` | Delete all sessions for a tool |

### Options

```
--tool <tool>    Target tool: claude-code, codex, droid
--yes, -y        Skip confirmation prompt
```

`nuke --all` requires `--tool` when stdin/stdout is not a terminal.

## Session paths

| Tool | Default path | Env override |
|------|--------------|--------------|
| Claude Code | `~/.claude/projects` | `NUKE_MY_SESSIONS_CLAUDE_ROOT` |
| Codex | `~/.codex/sessions` | `NUKE_MY_SESSIONS_CODEX_ROOT` |
| Droid | `~/.factory/sessions` | `NUKE_MY_SESSIONS_DROID_ROOT` |

## Development

```sh
cargo fmt && cargo test && cargo clippy
```

## License

MIT
