use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::BaseDirs;
use serde::Deserialize;

use crate::model::session::{SessionEntry, Tool};

use super::{
    DeleteSummary, SessionSource, delete_entries_within_root, project_from_cwd,
    sort_sessions_by_project,
};

const ROOT_ENV: &str = "NUKE_MY_SESSIONS_CLAUDE_ROOT";

pub struct ClaudeCodeSource {
    root: PathBuf,
}

impl ClaudeCodeSource {
    pub fn new() -> Result<Self> {
        let root = match env::var_os(ROOT_ENV) {
            Some(root) => PathBuf::from(root),
            None => default_root()?,
        };

        Ok(Self::at(root))
    }

    pub(crate) fn at(root: PathBuf) -> Self {
        Self { root }
    }

    fn read_session(&self, path: PathBuf) -> Result<SessionEntry> {
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

        let updated_at = fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .ok();
        let id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("unknown")
            .to_owned();

        let project = project_from_cwd(cwd.as_deref());

        Ok(SessionEntry {
            tool: Tool::ClaudeCode,
            id,
            project,
            path,
            updated_at,
        })
    }
}

impl SessionSource for ClaudeCodeSource {
    fn list_sessions(&self) -> Result<Vec<SessionEntry>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        for entry in fs::read_dir(&self.root)
            .with_context(|| format!("failed to read {}", self.root.display()))?
        {
            let path = entry?.path();
            if !path.is_dir() {
                continue;
            }

            for session in
                fs::read_dir(&path).with_context(|| format!("failed to read {}", path.display()))?
            {
                let session = session?.path();
                if session.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
                    continue;
                }

                sessions.push(self.read_session(session)?);
            }
        }

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

fn default_root() -> Result<PathBuf> {
    let home = BaseDirs::new()
        .context("failed to resolve home directory")?
        .home_dir()
        .to_path_buf();

    Ok(home.join(".claude").join("projects"))
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
