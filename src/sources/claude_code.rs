use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::model::session::{SessionEntry, Tool};

use super::{
    DeleteSummary, SessionSource, collect_jsonl_files, configured_root, delete_entries_within_root,
    project_from_cwd, session_file_id, session_updated_at, sort_sessions_by_project,
};

const ROOT_ENV: &str = "NUKE_MY_SESSIONS_CLAUDE_ROOT";

pub struct ClaudeCodeSource {
    root: PathBuf,
}

impl ClaudeCodeSource {
    pub fn new() -> Result<Self> {
        configured_root(ROOT_ENV, &[".claude", "projects"]).map(Self::at)
    }

    pub(crate) fn at(root: PathBuf) -> Self {
        Self { root }
    }

    fn read_session(path: PathBuf) -> Result<SessionEntry> {
        let file =
            fs::File::open(&path).with_context(|| format!("failed to open {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut cwd = None;

        for line in reader.lines() {
            let line = line?;
            let record: ClaudeRecord = match serde_json::from_str(&line) {
                Ok(record) => record,
                Err(_) => continue,
            };

            if record.cwd.is_some() {
                cwd = record.cwd;
                break;
            }
        }

        let project = project_from_cwd(cwd.as_deref());
        let updated_at = session_updated_at(&path);

        Ok(SessionEntry {
            tool: Tool::ClaudeCode,
            id: session_file_id(&path),
            project,
            path,
            updated_at,
        })
    }
}

impl SessionSource for ClaudeCodeSource {
    fn list_sessions(&self) -> Result<Vec<SessionEntry>> {
        let mut sessions = collect_jsonl_files(&self.root)?
            .into_iter()
            .map(Self::read_session)
            .collect::<Result<Vec<_>>>()?;

        sort_sessions_by_project(&mut sessions);
        Ok(sessions)
    }

    fn delete_sessions(&self, sessions: &[SessionEntry]) -> Result<DeleteSummary> {
        delete_entries_within_root(&self.root, sessions)
    }
}

#[derive(Deserialize)]
struct ClaudeRecord {
    #[serde(default)]
    cwd: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::ClaudeCodeSource;
    use crate::sources::SessionSource;

    #[test]
    fn lists_claude_sessions_from_project_directories() {
        let temp = tempdir().unwrap();
        let root = temp.path().join(".claude").join("projects");
        let project = root.join("repo-sandbox");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("session-1.jsonl"),
            concat!(
                "{\"type\":\"user\",\"message\":{\"content\":\"install rust\"},\"cwd\":\"~/repo/sandbox\"}\n",
                "{\"type\":\"assistant\",\"message\":{\"content\":\"ok\"},\"cwd\":\"~/repo/sandbox\"}\n"
            ),
        )
        .unwrap();
        let sessions = ClaudeCodeSource::at(root).list_sessions().unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].project.as_deref(), Some("sandbox"));
        assert_eq!(sessions[0].id, "session-1");
    }
}
