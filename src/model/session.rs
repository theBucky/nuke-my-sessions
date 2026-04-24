use std::fmt::{self, Display, Formatter};
use std::ops::Range;
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

pub(crate) struct ProjectGroup<'a> {
    pub project: &'a str,
    pub sessions: &'a [SessionEntry],
}

// Sessions must already be sorted so equal project names stay contiguous.
pub(crate) fn project_groups(
    sessions: &[SessionEntry],
) -> impl Iterator<Item = ProjectGroup<'_>> + '_ {
    sessions
        .chunk_by(|left, right| left.project_name() == right.project_name())
        .map(|sessions| ProjectGroup {
            project: sessions[0].project_name(),
            sessions,
        })
}

pub(crate) fn project_group_range_at(sessions: &[SessionEntry], current: usize) -> Range<usize> {
    project_start_at(sessions, current)..project_end_at(sessions, current)
}

fn project_start_at(sessions: &[SessionEntry], current: usize) -> usize {
    let project = sessions[current].project_name();
    sessions[..=current]
        .iter()
        .rposition(|session| session.project_name() != project)
        .map_or(0, |index| index + 1)
}

fn project_end_at(sessions: &[SessionEntry], current: usize) -> usize {
    let project = sessions[current].project_name();
    sessions[current..]
        .iter()
        .position(|session| session.project_name() != project)
        .map_or(sessions.len(), |index| current + index)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{SessionEntry, Tool, project_group_range_at, project_groups};

    #[test]
    fn groups_adjacent_sessions_by_project() {
        let sessions = vec![
            session("a-1", Some("a")),
            session("a-2", Some("a")),
            session("b-1", Some("b")),
            session("c-1", None),
        ];

        let groups = project_groups(&sessions)
            .map(|group| {
                (
                    group.project.to_owned(),
                    group
                        .sessions
                        .iter()
                        .map(|session| session.id.clone())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            groups,
            vec![
                (
                    String::from("a"),
                    vec![String::from("a-1"), String::from("a-2")]
                ),
                (String::from("b"), vec![String::from("b-1")]),
                (String::from("no project"), vec![String::from("c-1")]),
            ]
        );
    }

    #[test]
    fn finds_project_group_ranges() {
        let sessions = vec![
            session("a-1", Some("a")),
            session("a-2", Some("a")),
            session("b-1", Some("b")),
            session("c-1", None),
            session("c-2", None),
        ];

        assert_eq!(project_group_range_at(&sessions, 1), 0..2);
        assert_eq!(project_group_range_at(&sessions, 2), 2..3);
        assert_eq!(project_group_range_at(&sessions, 3), 3..5);
    }

    fn session(id: &str, project: Option<&str>) -> SessionEntry {
        SessionEntry {
            tool: Tool::Codex,
            id: id.to_owned(),
            project: project.map(str::to_owned),
            path: PathBuf::from(format!("{id}.jsonl")),
            updated_at: None,
        }
    }
}
