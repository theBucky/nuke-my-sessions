use std::path::PathBuf;

use anyhow::Result;
use serde::Deserialize;

use crate::model::session::{SessionEntry, Tool};

use super::{
    DeleteSummary, SessionSource, collect_jsonl_files, configured_root, delete_entries_within_root,
    first_jsonl_record, project_from_cwd, session_file_id, session_updated_at,
    sort_sessions_by_project,
};

const ROOT_ENV: &str = "NUKE_MY_SESSIONS_CODEX_ROOT";

pub struct CodexSource {
    root: PathBuf,
}

impl CodexSource {
    pub fn new() -> Result<Self> {
        configured_root(ROOT_ENV, &[".codex", "sessions"]).map(Self::at)
    }

    pub(crate) fn at(root: PathBuf) -> Self {
        Self { root }
    }

    fn read_session(path: PathBuf) -> Result<SessionEntry> {
        let session_meta =
            first_jsonl_record::<CodexRecord, SessionMetaPayload>(&path, |record| match record {
                CodexRecord::SessionMeta { payload } => Some(payload),
                CodexRecord::Other => None,
            })?;
        let id = session_meta
            .as_ref()
            .map_or_else(|| session_file_id(&path), |meta| meta.id.clone());
        let project = project_from_cwd(session_meta.and_then(|meta| meta.cwd).as_deref());
        let updated_at = session_updated_at(&path);

        Ok(SessionEntry {
            tool: Tool::Codex,
            id,
            project,
            path,
            updated_at,
        })
    }
}

impl SessionSource for CodexSource {
    fn count_sessions(&self) -> Result<usize> {
        collect_jsonl_files(&self.root).map(|paths| paths.len())
    }

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
#[serde(tag = "type")]
enum CodexRecord {
    #[serde(rename = "session_meta")]
    SessionMeta { payload: SessionMetaPayload },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct SessionMetaPayload {
    id: String,
    #[serde(default)]
    cwd: Option<PathBuf>,
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
            root.join("rollout-abc.jsonl"),
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"abc\",\"cwd\":\"~/repo/project\"}}\n",
        )
        .unwrap();
        let sessions = CodexSource::at(temp.path().join(".codex").join("sessions"))
            .list_sessions()
            .unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].project.as_deref(), Some("project"));
        assert_eq!(sessions[0].id, "abc");
    }
}
