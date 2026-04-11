use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::BaseDirs;
use serde::Deserialize;
use serde_json::Value;

use crate::session::{SessionEntry, Tool};

use super::{DeleteSummary, SessionSource, delete_entries_within_root};

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
        let mut first_prompt = None;

        for line in reader.lines() {
            let line = line?;
            let record: ClaudeRecord = match serde_json::from_str(&line) {
                Ok(record) => record,
                Err(_) => continue,
            };

            if cwd.is_none() {
                cwd = record.cwd;
            }

            if first_prompt.is_none() && record.record_type == "user" {
                first_prompt = record
                    .message
                    .as_ref()
                    .and_then(|message| extract_message_text(&message.content));
            }

            if cwd.is_some() && first_prompt.is_some() {
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

        Ok(SessionEntry {
            tool: Tool::ClaudeCode,
            id: id.clone(),
            label: build_label(cwd.as_deref(), first_prompt.as_deref(), &id),
            path,
            updated_at,
        })
    }
}

impl SessionSource for ClaudeCodeSource {
    fn tool(&self) -> Tool {
        Tool::ClaudeCode
    }

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

        sort_sessions(&mut sessions);
        Ok(sessions)
    }

    fn delete_sessions(&self, sessions: &[SessionEntry]) -> Result<DeleteSummary> {
        delete_entries_within_root(&self.root, sessions)
    }
}

#[derive(Deserialize)]
struct ClaudeRecord {
    #[serde(rename = "type")]
    record_type: String,
    #[serde(default)]
    message: Option<ClaudeMessage>,
    #[serde(default)]
    cwd: Option<PathBuf>,
}

#[derive(Deserialize)]
struct ClaudeMessage {
    content: Value,
}

fn default_root() -> Result<PathBuf> {
    let home = BaseDirs::new()
        .context("failed to resolve home directory")?
        .home_dir()
        .to_path_buf();

    Ok(home.join(".claude").join("projects"))
}

fn extract_message_text(content: &Value) -> Option<String> {
    let text = match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    };

    let text = text.trim().replace('\n', " ");
    if text.is_empty() {
        return None;
    }

    Some(truncate(&text, 72))
}

fn build_label(cwd: Option<&Path>, prompt: Option<&str>, id: &str) -> String {
    let project = cwd
        .and_then(|cwd| cwd.file_name())
        .and_then(|name| name.to_str());

    match (project, prompt) {
        (Some(project), Some(prompt)) => format!("{project}: {prompt}"),
        (Some(project), None) => project.to_owned(),
        (None, Some(prompt)) => prompt.to_owned(),
        (None, None) => id.to_owned(),
    }
}

fn truncate(value: &str, max_len: usize) -> String {
    if value.chars().count() <= max_len {
        return value.to_owned();
    }

    let truncated = value.chars().take(max_len - 3).collect::<String>();
    format!("{truncated}...")
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

    use super::ClaudeCodeSource;
    use crate::sources::SessionSource;

    #[test]
    fn lists_claude_sessions_from_project_directories() {
        let temp = tempdir().unwrap();
        let root = temp.path().join(".claude").join("projects");
        let project = root.join("-Users-m5pbook-repo-sandbox");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("session-1.jsonl"),
            concat!(
                "{\"type\":\"user\",\"message\":{\"content\":\"install rust\"},\"cwd\":\"/Users/m5pbook/repo/sandbox\"}\n",
                "{\"type\":\"assistant\",\"message\":{\"content\":\"ok\"},\"cwd\":\"/Users/m5pbook/repo/sandbox\"}\n"
            ),
        )
        .unwrap();
        let sessions = ClaudeCodeSource::at(root).list_sessions().unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].label, "sandbox: install rust");
    }
}
