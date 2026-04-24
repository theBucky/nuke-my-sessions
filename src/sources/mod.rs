mod claude_code;
mod codex;
mod droid;

use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use directories::BaseDirs;
use serde::de::DeserializeOwned;

use crate::model::session::{SessionEntry, Tool};

pub use claude_code::ClaudeCodeSource;
pub use codex::CodexSource;
pub use droid::DroidSource;

pub trait SessionSource {
    fn count_sessions(&self) -> Result<usize> {
        self.list_sessions().map(|sessions| sessions.len())
    }

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
    let summary = sessions
        .iter()
        .fold(DeleteSummary::success(0), |mut summary, session| {
            match delete_session(&root, session) {
                Ok(()) => {
                    summary.deleted += 1;
                }
                Err(error) => summary.failed.push(DeleteFailure {
                    path: session.path.clone(),
                    error: format!("{error:#}"),
                }),
            }

            summary
        });

    Ok(summary)
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
    collect_matching_paths(root, is_jsonl_file)
}

pub(crate) fn first_jsonl_record<T, U>(
    path: &Path,
    mut map_record: impl FnMut(T) -> Option<U>,
) -> Result<Option<U>>
where
    T: DeserializeOwned,
{
    fold_jsonl_records(path, None, |_, record| match map_record(record) {
        Some(mapped) => ControlFlow::Break(Some(mapped)),
        None => ControlFlow::Continue(None),
    })
}

pub(crate) fn fold_jsonl_records<T, U>(
    path: &Path,
    initial: U,
    mut fold: impl FnMut(U, T) -> ControlFlow<U, U>,
) -> Result<U>
where
    T: DeserializeOwned,
{
    let file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut state = initial;

    for line in reader.lines() {
        let line = line?;
        let record: T = match serde_json::from_str(&line) {
            Ok(record) => record,
            Err(_) => continue,
        };

        state = match fold(state, record) {
            ControlFlow::Continue(state) => state,
            ControlFlow::Break(state) => return Ok(state),
        };
    }

    Ok(state)
}

pub(crate) fn configured_root(root_env: &str, default_segments: &[&str]) -> Result<PathBuf> {
    env::var_os(root_env)
        .map(PathBuf::from)
        .map_or_else(|| default_root(default_segments), Ok)
}

pub(crate) fn project_from_cwd(cwd: Option<&Path>) -> Option<String> {
    cwd.and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
}

pub(crate) fn session_file_id(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown")
        .to_owned()
}

pub(crate) fn session_updated_at(path: &Path) -> Option<SystemTime> {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
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

pub(crate) fn is_jsonl_file(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
}

pub(crate) fn path_metadata(path: &Path) -> Result<fs::Metadata> {
    fs::metadata(path).with_context(|| format!("failed to inspect {}", path.display()))
}

pub(crate) fn read_directory_paths(path: &Path) -> Result<Vec<PathBuf>> {
    fs::read_dir(path)
        .with_context(|| format!("failed to read {}", path.display()))?
        .map(|entry| entry.map(|entry| entry.path()).map_err(Into::into))
        .collect()
}

fn collect_matching_paths(
    root: &Path,
    include_path: impl Fn(&Path) -> bool + Copy,
) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    fs::read_dir(root)
        .with_context(|| format!("failed to read {}", root.display()))?
        .try_fold(Vec::new(), |mut paths, entry| {
            let path = entry?.path();
            if path_metadata(&path)?.is_dir() {
                paths.extend(collect_matching_paths(&path, include_path)?);
            } else if include_path(&path) {
                paths.push(path);
            }

            Ok(paths)
        })
}

fn default_root(path_segments: &[&str]) -> Result<PathBuf> {
    let mut path = BaseDirs::new()
        .context("failed to resolve home directory")?
        .home_dir()
        .to_path_buf();
    path.extend(path_segments);
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde::Deserialize;
    use tempfile::tempdir;

    use super::{delete_entries_within_root, first_jsonl_record};
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

    #[test]
    fn reads_first_matching_jsonl_record() {
        #[derive(Deserialize)]
        struct TestRecord {
            value: i32,
        }

        let temp = tempdir().unwrap();
        let path = temp.path().join("records.jsonl");
        fs::write(&path, "{invalid json}\n{\"value\":1}\n{\"value\":2}\n").unwrap();

        let record = first_jsonl_record::<TestRecord, i32>(&path, |record| {
            (record.value > 1).then_some(record.value)
        })
        .unwrap();

        assert_eq!(record, Some(2));
    }

    #[test]
    fn returns_none_when_jsonl_has_no_matching_record() {
        #[derive(Deserialize)]
        struct TestRecord {
            value: i32,
        }

        let temp = tempdir().unwrap();
        let path = temp.path().join("records.jsonl");
        fs::write(&path, "{\"value\":1}\n{\"value\":2}\n").unwrap();

        let record = first_jsonl_record::<TestRecord, i32>(&path, |record| {
            (record.value > 2).then_some(record.value)
        })
        .unwrap();

        assert_eq!(record, None);
    }
}
