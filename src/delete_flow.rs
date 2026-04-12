mod selector;

use anyhow::Result;
use dialoguer::{Confirm, Select, theme::ColorfulTheme};

use crate::DeleteOutcome;
use crate::model::session::{SessionEntry, Tool};
use crate::sources::SessionSource;

pub trait Prompter {
    fn select_sessions(&mut self, sessions: &[SessionEntry]) -> Result<Vec<usize>>;
    fn confirm_delete(&mut self, tool: Tool, selected_count: usize) -> Result<bool>;
}

#[derive(Default)]
pub struct DialoguerPrompter {
    theme: ColorfulTheme,
}

impl DialoguerPrompter {
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

    pub fn confirm_nuke_all(&mut self, tool: Tool, selected_count: usize) -> Result<bool> {
        self.confirm_delete(tool, selected_count)
    }
}

impl Prompter for DialoguerPrompter {
    fn select_sessions(&mut self, sessions: &[SessionEntry]) -> Result<Vec<usize>> {
        selector::select_grouped_sessions(sessions, &self.theme)
    }

    fn confirm_delete(&mut self, tool: Tool, selected_count: usize) -> Result<bool> {
        Confirm::with_theme(&self.theme)
            .with_prompt(format!("delete {selected_count} {tool} session(s)?"))
            .default(false)
            .interact()
            .map_err(Into::into)
    }
}

pub fn run_select_flow(
    source: &dyn SessionSource,
    prompter: &mut impl Prompter,
    skip_confirmation: bool,
) -> Result<DeleteOutcome> {
    let sessions = source.list_sessions()?;
    if sessions.is_empty() {
        return Ok(DeleteOutcome::NoSessionsFound);
    }

    let selected_indices = prompter.select_sessions(&sessions)?;
    if selected_indices.is_empty() {
        return Ok(DeleteOutcome::NoSessionsDeleted);
    }

    let selected = selected_indices
        .into_iter()
        .map(|index| sessions[index].clone())
        .collect::<Vec<_>>();

    if !skip_confirmation && !prompter.confirm_delete(source.tool(), selected.len())? {
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
    use std::path::PathBuf;

    use anyhow::Result;

    use super::{Prompter, run_select_flow};
    use crate::model::session::{SessionEntry, Tool};
    use crate::sources::{DeleteSummary, SessionSource};

    struct FakeSource {
        sessions: Vec<SessionEntry>,
        deleted: RefCell<Vec<String>>,
    }

    impl SessionSource for FakeSource {
        fn tool(&self) -> Tool {
            Tool::Codex
        }

        fn list_sessions(&self) -> Result<Vec<SessionEntry>> {
            Ok(self.sessions.clone())
        }

        fn delete_sessions(&self, sessions: &[SessionEntry]) -> Result<DeleteSummary> {
            self.deleted
                .borrow_mut()
                .extend(sessions.iter().map(|session| session.id.clone()));

            Ok(DeleteSummary::success(sessions.len()))
        }
    }

    struct StubPrompter {
        selected: Vec<usize>,
        confirmed: bool,
    }

    impl Prompter for StubPrompter {
        fn select_sessions(&mut self, _: &[SessionEntry]) -> Result<Vec<usize>> {
            Ok(self.selected.clone())
        }

        fn confirm_delete(&mut self, _: Tool, _: usize) -> Result<bool> {
            Ok(self.confirmed)
        }
    }

    #[test]
    fn deletes_selected_sessions_from_flow() {
        let source = FakeSource {
            sessions: vec![session("a"), session("b"), session("c")],
            deleted: RefCell::new(Vec::new()),
        };
        let mut prompter = StubPrompter {
            selected: vec![0, 2],
            confirmed: true,
        };

        let deleted = run_select_flow(&source, &mut prompter, false).unwrap();

        assert!(matches!(deleted, crate::DeleteOutcome::Deleted(2)));
        assert_eq!(source.deleted.borrow().as_slice(), &["a", "c"]);
    }

    #[test]
    fn reports_no_sessions_found_from_flow() {
        let source = FakeSource {
            sessions: Vec::new(),
            deleted: RefCell::new(Vec::new()),
        };
        let mut prompter = StubPrompter {
            selected: vec![0],
            confirmed: true,
        };

        let deleted = run_select_flow(&source, &mut prompter, false).unwrap();

        assert!(matches!(deleted, crate::DeleteOutcome::NoSessionsFound));
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
}
