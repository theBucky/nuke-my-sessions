use std::fs;
use std::io::ErrorKind;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::model::session::{SessionEntry, Tool};

use super::{
    DeleteSummary, SessionSource, configured_root, delete_entries_within_root_using, delete_entry,
    fold_jsonl_records, is_jsonl_file, path_metadata, project_from_cwd, read_directory_paths,
    session_file_id, session_updated_at, sort_sessions_by_project,
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

    fn read_session(metadata_paths: &[PathBuf], session_path: PathBuf) -> Result<SessionEntry> {
        let metadata = read_metadata(metadata_paths)?;
        let metadata_updated_at = metadata_paths.iter().fold(None, |latest, metadata_path| {
            latest.max(session_updated_at(metadata_path))
        });
        let project = project_from_cwd(metadata.cwd.as_deref());
        let updated_at = session_updated_at(&session_path).max(metadata_updated_at);

        Ok(SessionEntry {
            tool: Tool::ClaudeCode,
            id: metadata
                .session_id
                .unwrap_or_else(|| session_file_id(&session_path)),
            project,
            path: session_path,
            updated_at,
        })
    }

    fn session_entry_paths(&self) -> Result<Vec<PathBuf>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }

        read_directory_paths(&self.root)?
            .into_iter()
            .try_fold(Vec::new(), |mut paths, path| {
                paths.extend(session_paths_under(&path)?);
                Ok(paths)
            })
    }

    fn collect_sessions(&self) -> Result<Vec<SessionEntry>> {
        self.session_entry_paths()?
            .into_iter()
            .try_fold(Vec::new(), |mut sessions, path| {
                if let Some(session) = Self::read_session_entry(&path)? {
                    sessions.push(session);
                }

                Ok(sessions)
            })
    }

    fn read_session_entry(path: &Path) -> Result<Option<SessionEntry>> {
        let metadata_paths = session_metadata_paths(path)?;
        if metadata_paths.is_empty() {
            return Ok(None);
        }

        Self::read_session(&metadata_paths, path.to_path_buf()).map(Some)
    }
}

impl SessionSource for ClaudeCodeSource {
    fn count_sessions(&self) -> Result<usize> {
        self.session_entry_paths().map(|paths| paths.len())
    }

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

#[derive(Default)]
struct ClaudeMetadata {
    cwd: Option<PathBuf>,
    session_id: Option<String>,
}

impl ClaudeMetadata {
    fn combine(self, other: Self) -> Self {
        Self {
            cwd: self.cwd.or(other.cwd),
            session_id: self.session_id.or(other.session_id),
        }
    }

    fn is_complete(&self) -> bool {
        self.cwd.is_some() && self.session_id.is_some()
    }
}

impl From<ClaudeRecord> for ClaudeMetadata {
    fn from(record: ClaudeRecord) -> Self {
        Self {
            cwd: record.cwd,
            session_id: record.session_id,
        }
    }
}

fn read_metadata(metadata_paths: &[PathBuf]) -> Result<ClaudeMetadata> {
    metadata_paths
        .iter()
        .try_fold(ClaudeMetadata::default(), |metadata, metadata_path| {
            if metadata.is_complete() {
                return Ok(metadata);
            }

            read_metadata_file(metadata_path).map(|file_metadata| metadata.combine(file_metadata))
        })
}

fn is_session_entry(path: &Path) -> Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(is_jsonl_file(path)),
        Ok(metadata) if metadata.is_dir() => session_dir_has_metadata(path),
        Ok(_) => Ok(false),
        Err(_) => Ok(false),
    }
}

fn session_paths_under(path: &Path) -> Result<Vec<PathBuf>> {
    if is_session_entry(path)? {
        return Ok(vec![path.to_path_buf()]);
    }

    match fs::metadata(path) {
        Ok(metadata) if !metadata.is_dir() => return Ok(Vec::new()),
        Ok(_) => {}
        Err(_) => return Ok(Vec::new()),
    }

    read_directory_paths(path)?
        .into_iter()
        .try_fold(Vec::new(), |mut paths, child| {
            if is_session_entry(&child)? {
                paths.push(child);
            }

            Ok(paths)
        })
}

fn session_metadata_paths(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(if is_jsonl_file(path) {
            vec![path.to_path_buf()]
        } else {
            Vec::new()
        });
    }

    if !path.is_dir() {
        return Ok(Vec::new());
    }

    let mut jsonl_paths = subagent_jsonl_paths(path)?;
    jsonl_paths.sort();
    Ok(jsonl_paths)
}

fn session_dir_has_metadata(path: &Path) -> Result<bool> {
    let subagents = path.join("subagents");
    if !subagents.is_dir() {
        return Ok(false);
    }

    for path in read_directory_paths(&subagents)? {
        if is_metadata_jsonl_file(&path)? {
            return Ok(true);
        }
    }

    Ok(false)
}

fn read_metadata_file(path: &Path) -> Result<ClaudeMetadata> {
    fold_jsonl_records(
        path,
        ClaudeMetadata::default(),
        |metadata, record: ClaudeRecord| {
            let metadata = metadata.combine(record.into());
            if metadata.is_complete() {
                ControlFlow::Break(metadata)
            } else {
                ControlFlow::Continue(metadata)
            }
        },
    )
}

fn subagent_jsonl_paths(path: &Path) -> Result<Vec<PathBuf>> {
    let subagents = path.join("subagents");
    if !subagents.is_dir() {
        return Ok(Vec::new());
    }

    read_directory_paths(&subagents)?
        .into_iter()
        .try_fold(Vec::new(), |mut jsonl_paths, path| {
            if is_metadata_jsonl_file(&path)? {
                jsonl_paths.push(path);
            }

            Ok(jsonl_paths)
        })
}

fn is_metadata_jsonl_file(path: &Path) -> Result<bool> {
    path_metadata(path).map(|metadata| metadata.is_file() && is_jsonl_file(path))
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
