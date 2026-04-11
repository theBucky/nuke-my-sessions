use anyhow::Result;
use dialoguer::{Confirm, MultiSelect, Select, theme::ColorfulTheme};

use crate::session::{SessionEntry, Tool};
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
        let tools = [Tool::ClaudeCode, Tool::Codex];
        let selection = Select::with_theme(&self.theme)
            .with_prompt("tool")
            .items(["Claude Code", "Codex"])
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
        let items = sessions
            .iter()
            .map(SessionEntry::display_line)
            .collect::<Vec<_>>();
        let selected = MultiSelect::with_theme(&self.theme)
            .with_prompt("sessions")
            .items(&items)
            .interact()?;

        Ok(selected)
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
) -> Result<usize> {
    let sessions = source.list_sessions()?;
    if sessions.is_empty() {
        return Ok(0);
    }

    let selected_indices = prompter.select_sessions(&sessions)?;
    if selected_indices.is_empty() {
        return Ok(0);
    }

    let selected = selected_indices
        .into_iter()
        .map(|index| sessions[index].clone())
        .collect::<Vec<_>>();

    if !skip_confirmation && !prompter.confirm_delete(source.tool(), selected.len())? {
        return Ok(0);
    }

    source.delete_sessions(&selected)?.finish()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use anyhow::Result;

    use super::{Prompter, run_select_flow};
    use crate::session::{SessionEntry, Tool};
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
            sessions: vec![
                session("a", "alpha"),
                session("b", "beta"),
                session("c", "gamma"),
            ],
            deleted: RefCell::new(Vec::new()),
        };
        let mut prompter = StubPrompter {
            selected: vec![0, 2],
            confirmed: true,
        };

        let deleted = run_select_flow(&source, &mut prompter, false).unwrap();

        assert_eq!(deleted, 2);
        assert_eq!(source.deleted.borrow().as_slice(), &["a", "c"]);
    }

    fn session(id: &str, label: &str) -> SessionEntry {
        SessionEntry {
            tool: Tool::Codex,
            id: id.into(),
            label: label.into(),
            path: PathBuf::from(format!("{id}.jsonl")),
            updated_at: None,
        }
    }
}
