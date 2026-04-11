use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::BaseDirs;
use serde::Deserialize;

use crate::session::{SessionEntry, Tool};

use super::{DeleteSummary, SessionSource, collect_jsonl_files, delete_entries_within_root};

const ROOT_ENV: &str = "NUKE_MY_SESSIONS_CODEX_ROOT";

pub struct CodexSource {
    root: PathBuf,
    index_path: PathBuf,
}

impl CodexSource {
    pub fn new() -> Result<Self> {
        let root = match env::var_os(ROOT_ENV) {
            Some(root) => PathBuf::from(root),
            None => default_root()?,
        };
        let index_path = root
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.clone())
            .join("session_index.jsonl");

        Ok(Self::at(root, index_path))
    }

    pub(crate) fn at(root: PathBuf, index_path: PathBuf) -> Self {
        Self { root, index_path }
    }

    fn read_index(&self) -> Result<HashMap<String, String>> {
        if !self.index_path.exists() {
            return Ok(HashMap::new());
        }

        let file = fs::File::open(&self.index_path)
            .with_context(|| format!("failed to open {}", self.index_path.display()))?;
        let reader = BufReader::new(file);
        let mut index = HashMap::new();

        for line in reader.lines() {
            let line = line?;
            let record: IndexRecord = match serde_json::from_str(&line) {
                Ok(record) => record,
                Err(_) => continue,
            };
            index.insert(record.id, record.thread_name);
        }

        Ok(index)
    }

    fn read_session(&self, path: PathBuf, index: &HashMap<String, String>) -> Result<SessionEntry> {
        let file =
            fs::File::open(&path).with_context(|| format!("failed to open {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut id = None;
        let mut cwd = None;

        for line in reader.lines() {
            let line = line?;
            let record: SessionMetaRecord = match serde_json::from_str(&line) {
                Ok(record) => record,
                Err(_) => continue,
            };

            if record.record_type != "session_meta" {
                continue;
            }

            id = Some(record.payload.id);
            cwd = record.payload.cwd;
            break;
        }

        let updated_at = fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .ok();
        let id = id.unwrap_or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("unknown")
                .to_owned()
        });

        Ok(SessionEntry {
            tool: Tool::Codex,
            label: build_label(index.get(&id).map(String::as_str), cwd.as_deref(), &id),
            id,
            path,
            updated_at,
        })
    }
}

impl SessionSource for CodexSource {
    fn tool(&self) -> Tool {
        Tool::Codex
    }

    fn list_sessions(&self) -> Result<Vec<SessionEntry>> {
        let index = self.read_index()?;
        let mut sessions = collect_jsonl_files(&self.root)?
            .into_iter()
            .map(|path| self.read_session(path, &index))
            .collect::<Result<Vec<_>>>()?;

        sort_sessions(&mut sessions);
        Ok(sessions)
    }

    fn delete_sessions(&self, sessions: &[SessionEntry]) -> Result<DeleteSummary> {
        delete_entries_within_root(&self.root, sessions)
    }
}

#[derive(Deserialize)]
struct IndexRecord {
    id: String,
    thread_name: String,
}

#[derive(Deserialize)]
struct SessionMetaRecord {
    #[serde(rename = "type")]
    record_type: String,
    payload: SessionMetaPayload,
}

#[derive(Deserialize)]
struct SessionMetaPayload {
    id: String,
    #[serde(default)]
    cwd: Option<PathBuf>,
}

fn default_root() -> Result<PathBuf> {
    let home = BaseDirs::new()
        .context("failed to resolve home directory")?
        .home_dir()
        .to_path_buf();

    Ok(home.join(".codex").join("sessions"))
}

fn build_label(thread_name: Option<&str>, cwd: Option<&Path>, id: &str) -> String {
    let project = cwd
        .and_then(|cwd| cwd.file_name())
        .and_then(|name| name.to_str());

    match (thread_name, project) {
        (Some(thread_name), Some(project)) => format!("{thread_name} [{project}]"),
        (Some(thread_name), None) => thread_name.to_owned(),
        (None, Some(project)) => project.to_owned(),
        (None, None) => id.to_owned(),
    }
}

fn sort_sessions(sessions: &mut [SessionEntry]) {
    sessions.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.label.cmp(&right.label))
    });
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::CodexSource;
    use crate::sources::SessionSource;

    #[test]
    fn lists_codex_sessions_from_index_and_session_files() {
        let temp = tempdir().unwrap();
        let root = temp
            .path()
            .join(".codex")
            .join("sessions")
            .join("2026")
            .join("04")
            .join("11");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            temp.path().join(".codex").join("session_index.jsonl"),
            "{\"id\":\"abc\",\"thread_name\":\"review auth flow\"}\n",
        )
        .unwrap();
        fs::write(
            root.join("rollout-abc.jsonl"),
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"abc\",\"cwd\":\"/Users/m5pbook/repo/project\"}}\n",
        )
        .unwrap();
        let sessions = CodexSource::at(
            temp.path().join(".codex").join("sessions"),
            temp.path().join(".codex").join("session_index.jsonl"),
        )
        .list_sessions()
        .unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].label, "review auth flow [project]");
    }
}
