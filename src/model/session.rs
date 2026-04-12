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
