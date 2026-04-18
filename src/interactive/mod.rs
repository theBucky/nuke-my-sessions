mod tui;

use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::Result;
use dialoguer::{Confirm, Select, theme::ColorfulTheme};

use crate::DeleteOutcome;
use crate::model::session::{SessionEntry, Tool};
use crate::sources::SessionSource;

pub(crate) use tui::run_session_browser;

pub(crate) struct ToolSessions {
    pub tool: Tool,
    pub sessions: Vec<SessionEntry>,
}

#[derive(Default)]
pub struct Prompter {
    theme: ColorfulTheme,
}

impl Prompter {
    pub fn choose_tool(&self) -> Tool {
        let tools = Tool::all();
        let labels = tools.map(Tool::noun);
        let selection = Select::with_theme(&self.theme)
            .with_prompt("tool")
            .items(labels)
            .default(0)
            .interact()
            .unwrap_or(0);

        tools[selection]
    }

    pub fn confirm_nuke_all(&self, tool: Tool, selected_count: usize) -> Result<bool> {
        Confirm::with_theme(&self.theme)
            .with_prompt(format!("delete {selected_count} {tool} session(s)?"))
            .default(false)
            .interact()
            .map_err(Into::into)
    }
}

pub(crate) fn delete_selected_sessions(
    source: &dyn SessionSource,
    sessions: &[SessionEntry],
    selected_paths: &BTreeSet<PathBuf>,
) -> Result<DeleteOutcome> {
    if sessions.is_empty() {
        return Ok(DeleteOutcome::NoSessionsFound);
    }

    let selected = sessions
        .iter()
        .filter(|session| selected_paths.contains(&session.path))
        .cloned()
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Ok(DeleteOutcome::NoSessionsDeleted);
    }

    source
        .delete_sessions(&selected)?
        .finish()
        .map(DeleteOutcome::Deleted)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use anyhow::Result;

    use super::delete_selected_sessions;
    use crate::model::session::{SessionEntry, Tool};
    use crate::sources::{DeleteSummary, SessionSource};

    struct FakeSource {
        sessions: Vec<SessionEntry>,
        deleted: RefCell<Vec<PathBuf>>,
    }

    impl SessionSource for FakeSource {
        fn list_sessions(&self) -> Result<Vec<SessionEntry>> {
            Ok(self.sessions.clone())
        }

        fn delete_sessions(&self, sessions: &[SessionEntry]) -> Result<DeleteSummary> {
            self.deleted
                .borrow_mut()
                .extend(sessions.iter().map(|session| session.path.clone()));

            Ok(DeleteSummary::success(sessions.len()))
        }
    }

    #[test]
    fn deletes_selected_sessions() {
        let source = FakeSource {
            sessions: vec![session("a"), session("b"), session("c")],
            deleted: RefCell::new(Vec::new()),
        };
        let selected = BTreeSet::from([PathBuf::from("a.jsonl"), PathBuf::from("c.jsonl")]);

        let deleted = delete_selected_sessions(&source, &source.sessions, &selected).unwrap();

        assert!(matches!(deleted, crate::DeleteOutcome::Deleted(2)));
        assert_eq!(
            source.deleted.borrow().as_slice(),
            &[PathBuf::from("a.jsonl"), PathBuf::from("c.jsonl")]
        );
    }

    #[test]
    fn only_deletes_selected_path_when_ids_collide() {
        let source = FakeSource {
            sessions: vec![session_in("dup", "one"), session_in("dup", "two")],
            deleted: RefCell::new(Vec::new()),
        };
        let selected = BTreeSet::from([PathBuf::from("one/dup.jsonl")]);

        let deleted = delete_selected_sessions(&source, &source.sessions, &selected).unwrap();

        assert!(matches!(deleted, crate::DeleteOutcome::Deleted(1)));
        assert_eq!(
            source.deleted.borrow().as_slice(),
            &[PathBuf::from("one/dup.jsonl")]
        );
    }

    #[test]
    fn reports_no_sessions_found() {
        let source = FakeSource {
            sessions: Vec::new(),
            deleted: RefCell::new(Vec::new()),
        };
        let selected = BTreeSet::from([PathBuf::from("a.jsonl")]);

        let deleted = delete_selected_sessions(&source, &source.sessions, &selected).unwrap();

        assert!(matches!(deleted, crate::DeleteOutcome::NoSessionsFound));
        assert!(source.deleted.borrow().is_empty());
    }

    #[test]
    fn reports_no_sessions_deleted_without_selection() {
        let source = FakeSource {
            sessions: vec![session("a"), session("b")],
            deleted: RefCell::new(Vec::new()),
        };

        let deleted =
            delete_selected_sessions(&source, &source.sessions, &BTreeSet::new()).unwrap();

        assert!(matches!(deleted, crate::DeleteOutcome::NoSessionsDeleted));
        assert!(source.deleted.borrow().is_empty());
    }

    fn session(id: &str) -> SessionEntry {
        SessionEntry {
            tool: Tool::Codex,
            id: id.into(),
            project: None,
            path: PathBuf::from(format!("{id}.jsonl")),
            updated_at: None,
        }
    }

    fn session_in(id: &str, dir: &str) -> SessionEntry {
        SessionEntry {
            tool: Tool::Codex,
            id: id.into(),
            project: None,
            path: PathBuf::from(dir).join(format!("{id}.jsonl")),
            updated_at: None,
        }
    }
}
