mod claude_code;
mod codex;
mod droid;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::model::session::{SessionEntry, Tool};

pub use claude_code::ClaudeCodeSource;
pub use codex::CodexSource;
pub use droid::DroidSource;

pub trait SessionSource {
    fn list_sessions(&self) -> Result<Vec<SessionEntry>>;
    fn delete_sessions(&self, sessions: &[SessionEntry]) -> Result<DeleteSummary>;
}

pub struct SourceRegistry {
    claude_code: ClaudeCodeSource,
    codex: CodexSource,
    droid: DroidSource,
}

impl SourceRegistry {
    pub fn new() -> Result<Self> {
        Ok(Self {
            claude_code: ClaudeCodeSource::new()?,
            codex: CodexSource::new()?,
            droid: DroidSource::new()?,
        })
    }

    pub fn source(&self, tool: Tool) -> &dyn SessionSource {
        match tool {
            Tool::ClaudeCode => &self.claude_code,
            Tool::Codex => &self.codex,
            Tool::Droid => &self.droid,
        }
    }
}

#[derive(Debug)]
pub struct DeleteSummary {
    pub deleted: usize,
    pub failed: Vec<DeleteFailure>,
}

impl DeleteSummary {
    pub fn success(deleted: usize) -> Self {
        Self {
            deleted,
            failed: Vec::new(),
        }
    }

    pub fn finish(self) -> Result<usize> {
        if self.failed.is_empty() {
            return Ok(self.deleted);
        }

        let failures = self
            .failed
            .into_iter()
            .map(|failure| format!("{}: {}", failure.path.display(), failure.error))
            .collect::<Vec<_>>()
            .join(", ");

        bail!(
            "deleted {} session(s), failed to delete: {failures}",
            self.deleted
        );
    }
}

#[derive(Debug)]
pub struct DeleteFailure {
    pub path: PathBuf,
    pub error: String,
}

pub fn delete_entries_within_root(root: &Path, sessions: &[SessionEntry]) -> Result<DeleteSummary> {
    delete_entries_within_root_using(root, sessions, |root, session| {
        delete_entry(root, &session.path)
    })
}

pub(crate) fn delete_entries_within_root_using(
    root: &Path,
    sessions: &[SessionEntry],
    mut delete_session: impl FnMut(&Path, &SessionEntry) -> Result<()>,
) -> Result<DeleteSummary> {
    if sessions.is_empty() {
        return Ok(DeleteSummary::success(0));
    }

    let root = fs::canonicalize(root)
        .with_context(|| format!("failed to resolve root {}", root.display()))?;
    let mut deleted = 0;
    let mut failed = Vec::new();

    for session in sessions {
        match delete_session(&root, session) {
            Ok(()) => deleted += 1,
            Err(error) => failed.push(DeleteFailure {
                path: session.path.clone(),
                error: format!("{error:#}"),
            }),
        }
    }

    Ok(DeleteSummary { deleted, failed })
}

pub(crate) fn delete_entry(root: &Path, path: &Path) -> Result<()> {
    let path =
        fs::canonicalize(path).with_context(|| format!("failed to resolve {}", path.display()))?;
    if !path.starts_with(root) {
        bail!(
            "refusing to delete {} outside {}",
            path.display(),
            root.display()
        );
    }

    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if metadata.is_dir() {
        fs::remove_dir_all(&path)
            .with_context(|| format!("failed to remove {}", path.display()))?;
        return Ok(());
    }

    fs::remove_file(&path).with_context(|| format!("failed to remove {}", path.display()))
}

pub fn collect_jsonl_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_jsonl_files_inner(root, &mut files)?;
    Ok(files)
}

pub(crate) fn project_from_cwd(cwd: Option<&Path>) -> Option<String> {
    cwd.and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
}

pub(crate) fn sort_sessions_by_project(sessions: &mut [SessionEntry]) {
    sessions.sort_by(|left, right| {
        left.project
            .is_none()
            .cmp(&right.project.is_none())
            .then_with(|| left.project.cmp(&right.project))
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn collect_jsonl_files_inner(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_jsonl_files_inner(&path, files)?;
            continue;
        }

        if path.extension().and_then(|extension| extension.to_str()) == Some("jsonl") {
            files.push(path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::delete_entries_within_root;
    use crate::model::session::{SessionEntry, Tool};

    #[test]
    fn refuses_to_delete_files_outside_root() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("root");
        let outside = temp.path().join("outside.jsonl");
        fs::create_dir_all(&root).unwrap();
        fs::write(&outside, "data").unwrap();

        let summary = delete_entries_within_root(
            &root,
            &[SessionEntry {
                tool: Tool::ClaudeCode,
                id: "outside".into(),
                project: None,
                path: outside.clone(),
                updated_at: None,
            }],
        )
        .unwrap();

        assert_eq!(summary.deleted, 0);
        assert_eq!(summary.failed.len(), 1);
        assert!(outside.exists());
    }
}
