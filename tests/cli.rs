use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn list_prints_sessions_from_all_tools() {
    let temp = tempdir().unwrap();
    let roots = TestRoots::new(temp.path());
    roots.write_claude_session("session-a", "install rust", "sandbox");
    roots.write_codex_session("session-b", "project");
    roots.write_droid_session("session-c", "factory-project");

    roots
        .command()
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Claude Code:"))
        .stdout(predicate::str::contains("[sandbox]"))
        .stdout(predicate::str::contains("session-a"))
        .stdout(predicate::str::contains("Codex:"))
        .stdout(predicate::str::contains("[project]"))
        .stdout(predicate::str::contains("session-b"))
        .stdout(predicate::str::contains("Droid:"))
        .stdout(predicate::str::contains("[factory-project]"))
        .stdout(predicate::str::contains("session-c"));
}

#[test]
fn nuke_all_yes_deletes_codex_sessions() {
    let temp = tempdir().unwrap();
    let roots = TestRoots::new(temp.path());
    let first = roots.write_codex_session("session-a", "project");
    let second = roots.write_codex_session("session-b", "project");

    roots
        .command()
        .args(["nuke", "--tool", "codex", "--all", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Codex: deleted 2 session(s)"));

    assert!(!first.exists());
    assert!(!second.exists());
}

#[test]
fn nuke_all_yes_deletes_droid_session_pairs() {
    let temp = tempdir().unwrap();
    let roots = TestRoots::new(temp.path());
    let first = roots.write_droid_session("session-a", "project");
    let second = roots.write_droid_session("session-b", "project");

    roots
        .command()
        .args(["nuke", "--tool", "droid", "--all", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Droid: deleted 2 session(s)"));

    assert!(!first.jsonl.exists());
    assert!(!first.settings.exists());
    assert!(!second.jsonl.exists());
    assert!(!second.settings.exists());
}

#[test]
fn nuke_all_yes_reports_when_no_sessions_exist() {
    let temp = tempdir().unwrap();
    let roots = TestRoots::new(temp.path());

    roots
        .command()
        .args(["nuke", "--tool", "codex", "--all", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Codex: no sessions found"));
}

#[test]
fn select_scoped_tool_reports_no_sessions_without_tty() {
    let temp = tempdir().unwrap();
    let roots = TestRoots::new(temp.path());

    roots
        .command()
        .args(["select", "--tool", "codex"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Codex: no sessions found"));
}

#[test]
fn select_scoped_tool_requires_tty_when_sessions_exist() {
    let temp = tempdir().unwrap();
    let roots = TestRoots::new(temp.path());
    roots.write_codex_session("session-a", "project");

    roots
        .command()
        .args(["select", "--tool", "codex"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "interactive mode requires a terminal",
        ));
}

struct TestRoots {
    claude: PathBuf,
    codex: PathBuf,
    droid: PathBuf,
}

impl TestRoots {
    fn new(root: &Path) -> Self {
        Self {
            claude: root.join(".claude").join("projects"),
            codex: root.join(".codex").join("sessions"),
            droid: root.join(".factory").join("sessions"),
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::cargo_bin("nuke-my-sessions").unwrap();
        command
            .env("NUKE_MY_SESSIONS_CLAUDE_ROOT", &self.claude)
            .env("NUKE_MY_SESSIONS_CODEX_ROOT", &self.codex)
            .env("NUKE_MY_SESSIONS_DROID_ROOT", &self.droid);
        command
    }

    fn write_claude_session(&self, id: &str, prompt: &str, project: &str) -> PathBuf {
        let project_root = self.claude.join(format!("repo-{project}"));
        fs::create_dir_all(&project_root).unwrap();
        let path = project_root.join(format!("{id}.jsonl"));
        fs::write(
            &path,
            format!(
                "{{\"type\":\"user\",\"message\":{{\"content\":\"{prompt}\"}},\"cwd\":\"{}\"}}\n",
                test_cwd(project)
            ),
        )
        .unwrap();

        path
    }

    fn write_codex_session(&self, id: &str, project: &str) -> PathBuf {
        let session_root = self.codex.join("2026").join("04").join("11");
        fs::create_dir_all(&session_root).unwrap();

        let path = session_root.join(format!("rollout-{id}.jsonl"));
        fs::write(
            &path,
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\",\"cwd\":\"{}\"}}}}\n",
                test_cwd(project)
            ),
        )
        .unwrap();

        path
    }

    fn write_droid_session(&self, id: &str, project: &str) -> DroidSessionFiles {
        let session_root = self.droid.join(format!("repo-{project}"));
        fs::create_dir_all(&session_root).unwrap();

        let jsonl = session_root.join(format!("{id}.jsonl"));
        let settings = session_root.join(format!("{id}.settings.json"));
        fs::write(
            &jsonl,
            format!(
                concat!(
                    "{{\"type\":\"session_start\",\"id\":\"{}\",",
                    "\"cwd\":\"{}\"}}\n",
                    "{{\"type\":\"message\",\"message\":{{\"role\":\"user\"}}}}\n"
                ),
                id,
                test_cwd(project)
            ),
        )
        .unwrap();
        fs::write(&settings, "{\"model\":\"custom:Revolt-GPT-5.4-0\"}\n").unwrap();

        DroidSessionFiles { jsonl, settings }
    }
}

struct DroidSessionFiles {
    jsonl: PathBuf,
    settings: PathBuf,
}

fn test_cwd(project: &str) -> String {
    format!("~/repo/{project}")
}
