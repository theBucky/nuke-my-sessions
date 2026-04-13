use std::fs;
use std::io::{BufRead, BufReader, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::model::session::{SessionEntry, Tool};

use super::{
    DeleteSummary, SessionSource, configured_root, delete_entries_within_root_using, delete_entry,
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

    fn read_session(metadata_paths: Vec<PathBuf>, session_path: PathBuf) -> Result<SessionEntry> {
        if metadata_paths.is_empty() {
            bail!("no metadata files found for {}", session_path.display());
        }

        let mut cwd = None;
        let mut session_id = None;
        let metadata_updated_at = metadata_paths.iter().fold(None, |latest, metadata_path| {
            latest_system_time(latest, session_updated_at(metadata_path))
        });

        for metadata_path in &metadata_paths {
            let file = fs::File::open(metadata_path)
                .with_context(|| format!("failed to open {}", metadata_path.display()))?;
            let reader = BufReader::new(file);

            for line in reader.lines() {
                let line = line?;
                let record: ClaudeRecord = match serde_json::from_str(&line) {
                    Ok(record) => record,
                    Err(_) => continue,
                };

                if session_id.is_none() {
                    session_id = record.session_id;
                }

                if cwd.is_none() {
                    cwd = record.cwd;
                }

                if cwd.is_some() && session_id.is_some() {
                    break;
                }
            }

            if cwd.is_some() && session_id.is_some() {
                break;
            }
        }

        let project = project_from_cwd(cwd.as_deref());
        let updated_at = latest_system_time(session_updated_at(&session_path), metadata_updated_at);

        Ok(SessionEntry {
            tool: Tool::ClaudeCode,
            id: session_id.unwrap_or_else(|| session_file_id(&session_path)),
            project,
            path: session_path,
            updated_at,
        })
    }

    fn collect_sessions(&self) -> Result<Vec<SessionEntry>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        for path in immediate_children(&self.root)? {
            if let Some(session) = Self::entry_session(&path)? {
                sessions.push(session);
                continue;
            }

            if path.is_dir() {
                Self::collect_project_sessions(&path, &mut sessions)?;
            }
        }

        Ok(sessions)
    }

    fn collect_project_sessions(
        project_root: &Path,
        sessions: &mut Vec<SessionEntry>,
    ) -> Result<()> {
        for path in immediate_children(project_root)? {
            if let Some(session) = Self::entry_session(&path)? {
                sessions.push(session);
            }
        }

        Ok(())
    }

    fn entry_session(path: &Path) -> Result<Option<SessionEntry>> {
        if path.is_file() {
            if !is_jsonl(path) {
                return Ok(None);
            }

            return Self::read_session(vec![path.to_path_buf()], path.to_path_buf()).map(Some);
        }

        if path.is_dir() {
            return Self::read_session_dir(path);
        }

        Ok(None)
    }

    fn read_session_dir(path: &Path) -> Result<Option<SessionEntry>> {
        let metadata_paths = session_dir_metadata_paths(path)?;
        if metadata_paths.is_empty() {
            return Ok(None);
        }

        Self::read_session(metadata_paths, path.to_path_buf()).map(Some)
    }
}

impl SessionSource for ClaudeCodeSource {
    fn list_sessions(&self) -> Result<Vec<SessionEntry>> {
        let mut sessions = self.collect_sessions()?;

        sort_sessions_by_project(&mut sessions);
        Ok(sessions)
    }

    fn delete_sessions(&self, sessions: &[SessionEntry]) -> Result<DeleteSummary> {
        delete_entries_within_root_using(&self.root, sessions, |root, session| {
            let prune_from = session.path.parent().map(PathBuf::from);
            delete_entry(root, &session.path)?;

            if let Some(prune_from) = prune_from {
                prune_empty_ancestors(root, &prune_from)?;
            }

            Ok(())
        })
    }
}

#[derive(Deserialize)]
struct ClaudeRecord {
    #[serde(default)]
    cwd: Option<PathBuf>,
    #[serde(rename = "sessionId", default)]
    session_id: Option<String>,
}

fn is_jsonl(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
}

fn session_dir_metadata_paths(path: &Path) -> Result<Vec<PathBuf>> {
    let subagents = path.join("subagents");
    if !subagents.is_dir() {
        return Ok(Vec::new());
    }

    let mut jsonl_paths = Vec::new();
    for entry in fs::read_dir(&subagents)
        .with_context(|| format!("failed to read {}", subagents.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if entry.metadata()?.is_file() && is_jsonl(&path) {
            jsonl_paths.push(path);
        }
    }

    jsonl_paths.sort();
    Ok(jsonl_paths)
}

fn latest_system_time(left: Option<SystemTime>, right: Option<SystemTime>) -> Option<SystemTime> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn prune_empty_ancestors(root: &Path, start: &Path) -> Result<()> {
    let mut current = match fs::canonicalize(start) {
        Ok(path) => path,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to resolve {}", start.display()));
        }
    };
    if !current.starts_with(root) {
        bail!(
            "refusing to prune {} outside {}",
            current.display(),
            root.display()
        );
    }

    loop {
        if current == root {
            break;
        }

        let Some(parent) = current.parent().map(Path::to_path_buf) else {
            break;
        };

        match fs::remove_dir(&current) {
            Ok(()) => current = parent,
            Err(error) if error.kind() == ErrorKind::DirectoryNotEmpty => break,
            Err(error) if error.kind() == ErrorKind::NotFound => break,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to remove {}", current.display()));
            }
        }
    }

    Ok(())
}

fn immediate_children(path: &Path) -> Result<Vec<PathBuf>> {
    fs::read_dir(path)
        .with_context(|| format!("failed to read {}", path.display()))?
        .map(|entry| entry.map(|entry| entry.path()).map_err(Into::into))
        .collect()
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

    #[test]
    fn lists_directory_backed_claude_sessions_once() {
        let temp = tempdir().unwrap();
        let root = temp.path().join(".claude").join("projects");
        let project = root.join("repo-sandbox");
        let session_dir = project.join("session-1");
        let subagents = session_dir.join("subagents");
        fs::create_dir_all(&subagents).unwrap();
        fs::write(
            subagents.join("agent-a.jsonl"),
            concat!(
                "{\"type\":\"user\",\"cwd\":\"~/repo/sandbox\",\"sessionId\":\"session-1\"}\n",
                "{\"type\":\"assistant\",\"cwd\":\"~/repo/sandbox\",\"sessionId\":\"session-1\"}\n"
            ),
        )
        .unwrap();
        fs::write(
            subagents.join("agent-a.meta.json"),
            "{\"agentId\":\"agent-a\"}\n",
        )
        .unwrap();
        fs::write(session_dir.join("tool-results.txt"), "ignored\n").unwrap();

        let sessions = ClaudeCodeSource::at(root).list_sessions().unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].project.as_deref(), Some("sandbox"));
        assert_eq!(sessions[0].id, "session-1");
        assert_eq!(sessions[0].path, session_dir);
    }

    #[test]
    fn deletes_directory_backed_sessions_and_prunes_empty_project_dirs() {
        let temp = tempdir().unwrap();
        let root = temp.path().join(".claude").join("projects");
        let project = root.join("repo-sandbox");
        let session_dir = project.join("session-1");
        let subagents = session_dir.join("subagents");
        let tool_results = session_dir.join("tool-results");
        fs::create_dir_all(&subagents).unwrap();
        fs::create_dir_all(&tool_results).unwrap();
        fs::write(
            subagents.join("agent-a.jsonl"),
            "{\"type\":\"user\",\"cwd\":\"~/repo/sandbox\",\"sessionId\":\"session-1\"}\n",
        )
        .unwrap();
        fs::write(
            subagents.join("agent-a.meta.json"),
            "{\"agentId\":\"agent-a\"}\n",
        )
        .unwrap();
        fs::write(tool_results.join("tool-1.txt"), "ok\n").unwrap();

        let source = ClaudeCodeSource::at(root.clone());
        let sessions = source.list_sessions().unwrap();
        let deleted = source.delete_sessions(&sessions).unwrap().finish().unwrap();

        assert_eq!(deleted, 1);
        assert!(!session_dir.exists());
        assert!(!project.exists());
        assert!(root.exists());
    }

    #[test]
    fn reads_directory_backed_session_metadata_across_subagent_logs() {
        let temp = tempdir().unwrap();
        let root = temp.path().join(".claude").join("projects");
        let project = root.join("repo-sandbox");
        let session_dir = project.join("session-1");
        let subagents = session_dir.join("subagents");
        fs::create_dir_all(&subagents).unwrap();
        fs::write(
            subagents.join("agent-a.jsonl"),
            "{\"type\":\"assistant\",\"message\":{\"content\":\"partial\"}}\n",
        )
        .unwrap();
        fs::write(
            subagents.join("agent-b.jsonl"),
            "{\"type\":\"user\",\"cwd\":\"~/repo/sandbox\",\"sessionId\":\"session-1\"}\n",
        )
        .unwrap();

        let sessions = ClaudeCodeSource::at(root).list_sessions().unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].project.as_deref(), Some("sandbox"));
        assert_eq!(sessions[0].id, "session-1");
    }

    #[test]
    fn prunes_empty_project_dirs_after_flat_session_deletion() {
        let temp = tempdir().unwrap();
        let root = temp.path().join(".claude").join("projects");
        let project = root.join("repo-sandbox");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("session-1.jsonl"),
            "{\"type\":\"user\",\"cwd\":\"~/repo/sandbox\",\"sessionId\":\"session-1\"}\n",
        )
        .unwrap();

        let source = ClaudeCodeSource::at(root.clone());
        let sessions = source.list_sessions().unwrap();
        let deleted = source.delete_sessions(&sessions).unwrap().finish().unwrap();

        assert_eq!(deleted, 1);
        assert!(!project.exists());
        assert!(root.exists());
    }
}
