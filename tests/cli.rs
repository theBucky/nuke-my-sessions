use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn list_prints_sessions_from_both_tools() {
    let temp = tempdir().unwrap();
    let roots = TestRoots::new(temp.path());
    roots.write_claude_session("session-a", "install rust", "sandbox");
    roots.write_codex_session("session-b", "project");

    Command::cargo_bin("nuke-my-sessions")
        .unwrap()
        .arg("list")
        .env("NUKE_MY_SESSIONS_CLAUDE_ROOT", &roots.claude_root)
        .env("NUKE_MY_SESSIONS_CODEX_ROOT", &roots.codex_root)
        .assert()
        .success()
        .stdout(predicate::str::contains("Claude Code:"))
        .stdout(predicate::str::contains("[sandbox]"))
        .stdout(predicate::str::contains("session-a"))
        .stdout(predicate::str::contains("Codex:"))
        .stdout(predicate::str::contains("[project]"))
        .stdout(predicate::str::contains("session-b"));
}

#[test]
fn nuke_all_yes_deletes_codex_sessions() {
    let temp = tempdir().unwrap();
    let roots = TestRoots::new(temp.path());
    let first = roots.write_codex_session("session-a", "project");
    let second = roots.write_codex_session("session-b", "project");

    Command::cargo_bin("nuke-my-sessions")
        .unwrap()
        .args(["nuke", "--tool", "codex", "--all", "--yes"])
        .env("NUKE_MY_SESSIONS_CLAUDE_ROOT", &roots.claude_root)
        .env("NUKE_MY_SESSIONS_CODEX_ROOT", &roots.codex_root)
        .assert()
        .success()
        .stdout(predicate::str::contains("Codex: deleted 2 session(s)"));

    assert!(!first.exists());
    assert!(!second.exists());
}

struct TestRoots {
    claude_root: PathBuf,
    codex_root: PathBuf,
}

impl TestRoots {
    fn new(root: &Path) -> Self {
        Self {
            claude_root: root.join(".claude").join("projects"),
            codex_root: root.join(".codex").join("sessions"),
        }
    }

    fn write_claude_session(&self, id: &str, prompt: &str, project: &str) -> PathBuf {
        let project_root = self
            .claude_root
            .join(format!("-Users-m5pbook-repo-{project}"));
        fs::create_dir_all(&project_root).unwrap();
        let path = project_root.join(format!("{id}.jsonl"));
        fs::write(
            &path,
            format!(
                "{{\"type\":\"user\",\"message\":{{\"content\":\"{prompt}\"}},\"cwd\":\"/Users/m5pbook/repo/{project}\"}}\n"
            ),
        )
        .unwrap();

        path
    }

    fn write_codex_session(&self, id: &str, project: &str) -> PathBuf {
        let session_root = self.codex_root.join("2026").join("04").join("11");
        fs::create_dir_all(&session_root).unwrap();

        let path = session_root.join(format!("rollout-{id}.jsonl"));
        fs::write(
            &path,
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\",\"cwd\":\"/Users/m5pbook/repo/{project}\"}}}}\n"
            ),
        )
        .unwrap();

        path
    }
}
