# nuke-my-sessions

CLI to list and delete local session history for Claude Code, Codex, and Droid.

## Run

```sh
cargo run --
cargo run -- list
cargo run -- nuke --tool codex --all --yes
```

## Build

```sh
cargo build
cargo build --release
```

## Usage

```sh
nuke-my-sessions
nuke-my-sessions select
nuke-my-sessions list
nuke-my-sessions list --tool codex
nuke-my-sessions nuke --tool droid --all
nuke-my-sessions nuke --tool codex --all --yes
```

`select` is default. It opens an interactive picker for one tool.

`nuke` only works with `--all`. If stdin/stdout is not a terminal, `nuke --all` also requires `--tool`.

## Tools

| Tool | `--tool` value | Default root | Env override |
|------|----------------|--------------|--------------|
| Claude Code | `claude-code` | `~/.claude/projects` | `NUKE_MY_SESSIONS_CLAUDE_ROOT` |
| Codex | `codex` | `~/.codex/sessions` | `NUKE_MY_SESSIONS_CODEX_ROOT` |
| Droid | `droid` | `~/.factory/sessions` | `NUKE_MY_SESSIONS_DROID_ROOT` |

## Behavior

* `list` prints sessions grouped by project.
* `select` lets you choose sessions to delete interactively.
* `nuke --all` deletes every session for one tool, with confirmation unless `--yes` is set.
* Droid deletion removes both the `.jsonl` session file and matching `.settings.json` file.

## License

MIT
