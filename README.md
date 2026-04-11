# nuke-my-sessions

Claude Code and Codex store session history locally but provide no native way to remove them. Codex offers archive, not deletion. This CLI fills the gap.

## Installation

```sh
cargo install --path .
```

## Usage

```sh
nuke-my-sessions              # interactive session selection (default)
nuke-my-sessions list         # list all sessions
nuke-my-sessions nuke --all   # delete all sessions for a tool (prompts for confirmation)
nuke-my-sessions nuke --all -y --tool codex   # skip confirmation, target specific tool
```

## Session locations

| Tool        | Path                 |
|-------------|----------------------|
| Claude Code | `~/.claude/projects` |
| Codex       | `~/.codex/sessions`  |

Override with `NUKE_MY_SESSIONS_CLAUDE_ROOT` or `NUKE_MY_SESSIONS_CODEX_ROOT`.

## License

MIT
