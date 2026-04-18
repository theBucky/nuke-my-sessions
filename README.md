# nuke-my-sessions

Delete local session history for Claude Code, Codex, and Droid.

## Installation

Download the prebuilt binary for your platform from the [latest release](https://github.com/theBucky/nuke-my-sessions/releases/tag/latest):

```sh
# macOS (Apple Silicon)
curl -L -o nuke-my-sessions https://github.com/theBucky/nuke-my-sessions/releases/download/latest/nuke-my-sessions-macos-arm64

# Linux (x86_64)
curl -L -o nuke-my-sessions https://github.com/theBucky/nuke-my-sessions/releases/download/latest/nuke-my-sessions-linux-amd64

chmod +x nuke-my-sessions
mv nuke-my-sessions /usr/local/bin/
```

On macOS, the binary is unsigned, so Gatekeeper will quarantine it on first run. Clear the attribute once after download:

```sh
xattr -d com.apple.quarantine /usr/local/bin/nuke-my-sessions
```

Or build from source:

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
