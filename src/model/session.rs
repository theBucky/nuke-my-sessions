use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;
use std::time::SystemTime;

use clap::ValueEnum;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, ValueEnum)]
pub enum Tool {
    #[value(name = "claude-code")]
    ClaudeCode,
    #[value(name = "codex")]
    Codex,
    #[value(name = "droid")]
    Droid,
}

impl Tool {
    pub const fn all() -> [Self; 3] {
        [Self::ClaudeCode, Self::Codex, Self::Droid]
    }

    pub fn noun(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::Droid => "Droid",
        }
    }
}

impl Display for Tool {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.noun())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionEntry {
    pub tool: Tool,
    pub id: String,
    pub project: Option<String>,
    pub path: PathBuf,
    pub updated_at: Option<SystemTime>,
}

impl SessionEntry {
    pub fn project_name(&self) -> &str {
        self.project.as_deref().unwrap_or("no project")
    }

    pub fn display_line(&self) -> &str {
        &self.id
    }
}

// Sessions must already be sorted so equal project names stay contiguous.
pub(crate) fn for_each_project_group(
    sessions: &[SessionEntry],
    mut visit: impl FnMut(&str, &[SessionEntry]),
) {
    let mut start = 0;
    while start < sessions.len() {
        let project = sessions[start].project_name();
        let mut end = start + 1;

        while end < sessions.len() && sessions[end].project_name() == project {
            end += 1;
        }

        visit(project, &sessions[start..end]);
        start = end;
    }
}
