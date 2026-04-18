use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;

use crate::model::session::{SessionEntry, Tool};

use super::{
    DeleteSummary, SessionSource, collect_jsonl_files, configured_root,
    delete_entries_within_root_using, delete_entry, first_jsonl_record, project_from_cwd,
    session_file_id, session_updated_at, sort_sessions_by_project,
};

const ROOT_ENV: &str = "NUKE_MY_SESSIONS_DROID_ROOT";

pub struct DroidSource {
    root: PathBuf,
}

impl DroidSource {
    pub fn new() -> Result<Self> {
        configured_root(ROOT_ENV, &[".factory", "sessions"]).map(Self::at)
    }

    pub(crate) fn at(root: PathBuf) -> Self {
        Self { root }
    }

    fn read_session(path: PathBuf) -> Result<SessionEntry> {
        let session_start = first_jsonl_record::<DroidRecord, (Option<String>, Option<PathBuf>)>(
            &path,
            |record| match record {
                DroidRecord::SessionStart { id, cwd } => Some((id, cwd)),
                DroidRecord::Other => None,
            },
        )?;
        let updated_at = session_updated_at(&path);
        let id = session_start
            .as_ref()
            .and_then(|(id, _)| id.clone())
            .unwrap_or_else(|| session_file_id(&path));
        let project = project_from_cwd(session_start.and_then(|(_, cwd)| cwd).as_deref());

        Ok(SessionEntry {
            tool: Tool::Droid,
            id,
            project,
            path,
            updated_at,
        })
    }
}

impl SessionSource for DroidSource {
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
        delete_entries_within_root_using(&self.root, sessions, |root, session| {
            delete_session_pair(root, &session.path)
        })
    }
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum DroidRecord {
    #[serde(rename = "session_start")]
    SessionStart {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        cwd: Option<PathBuf>,
    },
    #[serde(other)]
    Other,
}

fn delete_session_pair(root: &Path, jsonl_path: &Path) -> Result<()> {
    let settings_path = jsonl_path.with_extension("settings.json");
    if let Err(error) = delete_entry(root, &settings_path)
        && !is_not_found(&error)
    {
        return Err(error);
    }

    delete_entry(root, jsonl_path)
}

fn is_not_found(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io_error| io_error.kind() == ErrorKind::NotFound)
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::DroidSource;
    use crate::sources::SessionSource;

    #[test]
    fn lists_droid_sessions_from_session_start_records() {
        let temp = tempdir().unwrap();
        let root = temp.path().join(".factory").join("sessions");
        let project = root.join("repo-sandbox");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("session-1.jsonl"),
            concat!(
                "{\"type\":\"session_start\",\"id\":\"session-1\",\"cwd\":\"~/repo/sandbox\"}\n",
                "{\"type\":\"message\",\"message\":{\"role\":\"user\"}}\n"
            ),
        )
        .unwrap();
        fs::write(
            project.join("session-1.settings.json"),
            "{\"model\":\"gpt\"}\n",
        )
        .unwrap();

        let sessions = DroidSource::at(root).list_sessions().unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].project.as_deref(), Some("sandbox"));
        assert_eq!(sessions[0].id, "session-1");
    }

    #[test]
    fn deletes_droid_jsonl_and_settings_files_together() {
        let temp = tempdir().unwrap();
        let root = temp.path().join(".factory").join("sessions");
        let project = root.join("repo-sandbox");
        fs::create_dir_all(&project).unwrap();
        let jsonl_path = project.join("session-1.jsonl");
        let settings_path = project.join("session-1.settings.json");
        fs::write(
            &jsonl_path,
            "{\"type\":\"session_start\",\"id\":\"session-1\",\"cwd\":\"~/repo/sandbox\"}\n",
        )
        .unwrap();
        fs::write(&settings_path, "{\"model\":\"gpt\"}\n").unwrap();

        let source = DroidSource::at(root);
        let sessions = source.list_sessions().unwrap();
        let deleted = source.delete_sessions(&sessions).unwrap().finish().unwrap();

        assert_eq!(deleted, 1);
        assert!(!jsonl_path.exists());
        assert!(!settings_path.exists());
    }

    #[test]
    fn deletes_droid_jsonl_when_settings_file_is_missing() {
        let temp = tempdir().unwrap();
        let root = temp.path().join(".factory").join("sessions");
        let project = root.join("repo-sandbox");
        fs::create_dir_all(&project).unwrap();
        let jsonl_path = project.join("session-1.jsonl");
        fs::write(
            &jsonl_path,
            "{\"type\":\"session_start\",\"id\":\"session-1\",\"cwd\":\"~/repo/sandbox\"}\n",
        )
        .unwrap();

        let source = DroidSource::at(root);
        let sessions = source.list_sessions().unwrap();
        let deleted = source.delete_sessions(&sessions).unwrap().finish().unwrap();

        assert_eq!(deleted, 1);
        assert!(!jsonl_path.exists());
    }
}
