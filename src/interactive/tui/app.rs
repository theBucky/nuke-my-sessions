use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{
    AppEvent, EMPTY_SELECTION_STATUS, Focus, LoadState, NO_SESSIONS_STATUS, RowCache,
    SessionBrowser, ToolState,
};
use crate::DeleteOutcome;
use crate::model::session::{SessionEntry, Tool, for_each_project_group};
use crate::sources::SourceRegistry;

impl<'a> SessionBrowser<'a> {
    pub(super) fn new(
        registry: &'a SourceRegistry,
        tool_sessions: Option<super::super::ToolSessions>,
        skip_confirmation: bool,
    ) -> Self {
        let is_scoped = tool_sessions.is_some();
        let tools = match tool_sessions {
            Some(super::super::ToolSessions { tool, sessions }) => {
                vec![ToolState::loaded(tool, sessions)]
            }
            None => Tool::all().into_iter().map(ToolState::unloaded).collect(),
        };
        let mut app = Self {
            registry,
            tools,
            active_tool: 0,
            focus: if is_scoped {
                Focus::Sessions
            } else {
                Focus::Tools
            },
            pending_delete: false,
            skip_confirmation,
            status: None,
        };

        if !is_scoped {
            app.load_tool_counts();
            match app.load_active_tool() {
                Ok(()) => {}
                Err(error) => app.status = Some(format!("{}: {error}", app.current_tool().tool)),
            }
        }
        app.sync_status_with_active_tool();
        app
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) -> Result<AppEvent> {
        if is_ctrl_c(key) {
            return Ok(AppEvent::Quit);
        }

        match key.code {
            KeyCode::Esc => self.clear_pending_delete(),
            KeyCode::Tab => self.toggle_focus(),
            KeyCode::Up => self.move_focus_cursor(-1),
            KeyCode::Down => self.move_focus_cursor(1),
            KeyCode::Char('j') => self.move_project_cursor(1),
            KeyCode::Char('k') => self.move_project_cursor(-1),
            KeyCode::Char('a') => self.toggle_all_current_tool(),
            KeyCode::Char(' ') => self.toggle_current_session(),
            KeyCode::Enter => return self.handle_enter(),
            _ => {}
        }

        Ok(AppEvent::Continue)
    }

    fn handle_enter(&mut self) -> Result<AppEvent> {
        if self.focus == Focus::Sessions {
            return self.delete_current_selection();
        }

        self.clear_pending_delete();
        self.focus = Focus::Sessions;
        Ok(AppEvent::Continue)
    }

    fn move_focus_cursor(&mut self, direction: isize) {
        self.clear_pending_delete();

        match self.focus {
            Focus::Tools => self.move_tool_cursor(direction),
            Focus::Sessions => self.current_tool_mut().move_cursor(direction),
        }
    }

    fn move_project_cursor(&mut self, direction: isize) {
        if self.focus != Focus::Sessions {
            return;
        }

        self.clear_pending_delete();
        self.current_tool_mut().move_project(direction);
    }

    fn toggle_all_current_tool(&mut self) {
        if self.focus != Focus::Sessions || self.current_tool().sessions.is_empty() {
            return;
        }

        self.clear_pending_delete();
        self.current_tool_mut().toggle_all_selected();
        self.status = None;
    }

    fn toggle_current_session(&mut self) {
        if self.focus != Focus::Sessions {
            return;
        }

        self.clear_pending_delete();
        self.current_tool_mut().toggle_selected();
        self.status = None;
    }

    fn toggle_focus(&mut self) {
        self.clear_pending_delete();
        self.focus = self.focus.next();
    }

    fn delete_current_selection(&mut self) -> Result<AppEvent> {
        let tool = self.current_tool().tool;
        if self.current_tool().selected.is_empty() {
            self.clear_pending_delete();
            self.status = Some(String::from(EMPTY_SELECTION_STATUS));
            return Ok(AppEvent::Continue);
        }

        if !self.skip_confirmation && !self.pending_delete {
            self.pending_delete = true;
            return Ok(AppEvent::Continue);
        }

        let outcome = {
            let tool_state = self.current_tool();
            super::super::delete_selected_sessions(
                self.registry.source(tool),
                &tool_state.sessions,
                &tool_state.selected,
            )?
        };
        self.clear_pending_delete();

        match outcome {
            DeleteOutcome::Deleted(deleted) => {
                let sessions = self.registry.source(tool).list_sessions();
                let status = reload_deleted_sessions(self.current_tool_mut(), deleted, sessions);
                self.status = Some(status);
                Ok(AppEvent::Continue)
            }
            DeleteOutcome::NoSessionsFound => {
                self.status = Some(String::from(NO_SESSIONS_STATUS));
                Ok(AppEvent::Continue)
            }
            DeleteOutcome::NoSessionsDeleted => {
                self.status = Some(String::from(EMPTY_SELECTION_STATUS));
                Ok(AppEvent::Continue)
            }
        }
    }

    fn move_tool_cursor(&mut self, direction: isize) {
        let next = offset_index(self.active_tool, direction, self.tools.len());
        if next == self.active_tool {
            return;
        }

        self.active_tool = next;
        match self.load_active_tool() {
            Ok(()) => self.sync_status_with_active_tool(),
            Err(error) => self.status = Some(format!("{}: {error}", self.current_tool().tool)),
        }
    }

    fn sync_status_with_active_tool(&mut self) {
        let status = {
            let tool_state = self.current_tool();
            match &tool_state.load_state {
                LoadState::Failed(error) => Some(format!("{}: {error}", tool_state.tool)),
                LoadState::Ready if tool_state.sessions.is_empty() => {
                    Some(String::from(NO_SESSIONS_STATUS))
                }
                LoadState::Ready | LoadState::Unloaded => None,
            }
        };
        self.status = status;
    }

    fn clear_pending_delete(&mut self) {
        self.pending_delete = false;
    }

    fn current_tool(&self) -> &ToolState {
        &self.tools[self.active_tool]
    }

    fn current_tool_mut(&mut self) -> &mut ToolState {
        &mut self.tools[self.active_tool]
    }

    fn load_active_tool(&mut self) -> Result<()> {
        self.tools[self.active_tool].load(self.registry)
    }

    fn load_tool_counts(&mut self) {
        for tool_state in &mut self.tools {
            match self.registry.source(tool_state.tool).count_sessions() {
                Ok(count) => tool_state.set_session_count(count),
                Err(_) => tool_state.set_count_failed(),
            }
        }
    }
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::Tools => Self::Sessions,
            Self::Sessions => Self::Tools,
        }
    }
}

impl ToolState {
    fn unloaded(tool: Tool) -> Self {
        Self {
            tool,
            sessions: Vec::new(),
            rows: Vec::new(),
            session_rows: Vec::new(),
            selected: BTreeSet::default(),
            cursor: 0,
            cursor_row: 0,
            session_count: None,
            count_failed: false,
            load_state: LoadState::Unloaded,
        }
    }

    fn loaded(tool: Tool, sessions: Vec<SessionEntry>) -> Self {
        let mut state = Self::unloaded(tool);
        state.set_sessions_ready(sessions);
        state
    }

    fn load(&mut self, registry: &SourceRegistry) -> Result<()> {
        if matches!(self.load_state, LoadState::Ready) {
            return Ok(());
        }

        match registry.source(self.tool).list_sessions() {
            Ok(sessions) => {
                self.set_sessions_ready(sessions);
                Ok(())
            }
            Err(error) => {
                self.set_load_failed(error.to_string());
                Err(error)
            }
        }
    }

    fn move_cursor(&mut self, direction: isize) {
        if self.sessions.is_empty() {
            self.set_cursor(0);
            return;
        }

        self.set_cursor(offset_index(self.cursor, direction, self.sessions.len()));
    }

    fn move_project(&mut self, direction: isize) {
        let next = match direction.cmp(&0) {
            std::cmp::Ordering::Greater => self.project_start_after(self.cursor),
            std::cmp::Ordering::Less => self.project_start_before(self.cursor),
            std::cmp::Ordering::Equal => None,
        };

        if let Some(cursor) = next {
            self.set_cursor(cursor);
        }
    }

    fn toggle_selected(&mut self) {
        let Some(session) = self.sessions.get(self.cursor) else {
            return;
        };

        if !self.selected.insert(session.path.clone()) {
            self.selected.remove(&session.path);
        }
    }

    fn toggle_all_selected(&mut self) {
        if self
            .sessions
            .iter()
            .all(|session| self.selected.contains(&session.path))
        {
            self.selected.clear();
            return;
        }

        self.selected = self
            .sessions
            .iter()
            .map(|session| session.path.clone())
            .collect();
    }

    pub(super) fn session_badge(&self) -> String {
        if matches!(self.load_state, LoadState::Failed(_)) || self.count_failed {
            return String::from("!");
        }

        self.session_count
            .map_or_else(|| String::from("-"), |count| count.to_string())
    }

    fn apply_row_cache(&mut self) {
        let row_cache = RowCache::build(&self.sessions);
        self.rows = row_cache.rows;
        self.session_rows = row_cache.session_rows;
        self.set_cursor(self.cursor);
    }

    fn retain_existing_selection(&mut self) {
        let current_paths: BTreeSet<PathBuf> = self
            .sessions
            .iter()
            .map(|session| session.path.clone())
            .collect();
        self.selected.retain(|path| current_paths.contains(path));
    }

    fn set_sessions_ready(&mut self, sessions: Vec<SessionEntry>) {
        self.session_count = Some(sessions.len());
        self.count_failed = false;
        self.sessions = sessions;
        self.apply_row_cache();
        self.retain_existing_selection();
        self.load_state = LoadState::Ready;
    }

    fn set_session_count(&mut self, count: usize) {
        self.session_count = Some(count);
        self.count_failed = false;
    }

    fn set_count_failed(&mut self) {
        self.session_count = None;
        self.count_failed = true;
    }

    fn set_load_failed(&mut self, error: impl Into<String>) {
        self.reset();
        self.load_state = LoadState::Failed(error.into());
    }

    fn set_cursor(&mut self, cursor: usize) {
        self.cursor = cursor.min(self.sessions.len().saturating_sub(1));
        self.cursor_row = self.session_rows.get(self.cursor).copied().unwrap_or(0);
    }

    fn project_start_after(&self, current: usize) -> Option<usize> {
        let current_project = self.sessions.get(current)?.project_name();
        self.sessions
            .iter()
            .enumerate()
            .skip(current + 1)
            .find(|(_, session)| session.project_name() != current_project)
            .map(|(index, _)| index)
    }

    fn project_start_before(&self, current: usize) -> Option<usize> {
        let current_project = self.sessions.get(current)?.project_name();
        let current_start = self.sessions[..=current]
            .iter()
            .rposition(|session| session.project_name() != current_project)
            .map_or(0, |index| index + 1);
        if current_start == 0 {
            return None;
        }

        let previous_project = self.sessions[current_start - 1].project_name();
        Some(
            self.sessions[..current_start]
                .iter()
                .rposition(|session| session.project_name() != previous_project)
                .map_or(0, |index| index + 1),
        )
    }

    fn reset(&mut self) {
        self.sessions.clear();
        self.rows.clear();
        self.session_rows.clear();
        self.selected.clear();
        self.cursor = 0;
        self.cursor_row = 0;
    }
}

impl RowCache {
    fn build(sessions: &[SessionEntry]) -> Self {
        let mut rows = Vec::new();
        let mut session_rows = Vec::with_capacity(sessions.len());
        let mut session_index = 0;

        for_each_project_group(sessions, |project, project_sessions| {
            rows.push(super::DisplayRow::Header(format!("  [{project}]")));
            for session in project_sessions {
                rows.push(super::DisplayRow::Session {
                    session_index,
                    text: session.display_line().to_owned(),
                });
                session_rows.push(rows.len().saturating_sub(1));
                session_index += 1;
            }
        });

        Self { rows, session_rows }
    }
}

fn reload_deleted_sessions(
    tool_state: &mut ToolState,
    deleted: usize,
    sessions: Result<Vec<SessionEntry>>,
) -> String {
    match sessions {
        Ok(sessions) => {
            tool_state.set_sessions_ready(sessions);
            format!("deleted {deleted} session(s)")
        }
        Err(error) => {
            tool_state.set_load_failed(error.to_string());
            format!("deleted {deleted} session(s), failed to refresh")
        }
    }
}

fn is_ctrl_c(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')
}

fn offset_index(current: usize, offset: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }

    current
        .saturating_add_signed(offset)
        .min(len.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use anyhow::anyhow;

    use super::{LoadState, ToolState, reload_deleted_sessions};
    use crate::model::session::{SessionEntry, Tool};

    #[test]
    fn keeps_delete_success_when_reload_fails() {
        let deleted_path = PathBuf::from("/tmp/deleted.jsonl");
        let retained_path = PathBuf::from("/tmp/retained.jsonl");
        let mut tool_state = ToolState::loaded(
            Tool::Codex,
            vec![
                session("deleted", deleted_path.clone()),
                session("retained", retained_path),
            ],
        );
        tool_state.selected.insert(deleted_path);
        let status = reload_deleted_sessions(&mut tool_state, 1, Err(anyhow!("boom")));

        assert_eq!(status, "deleted 1 session(s), failed to refresh");
        assert!(tool_state.sessions.is_empty());
        assert!(tool_state.selected.is_empty());
        assert!(matches!(
            tool_state.load_state,
            LoadState::Failed(ref error) if error == "boom"
        ));
    }

    #[test]
    fn jumps_between_project_starts() {
        let mut tool_state = ToolState::loaded(
            Tool::Codex,
            vec![
                session_in("a-1", "a"),
                session_in("a-2", "a"),
                session_in("b-1", "b"),
                session_in("b-2", "b"),
                session_in("c-1", "c"),
            ],
        );

        tool_state.move_cursor(1);
        tool_state.move_project(1);
        assert_eq!(tool_state.cursor, 2);

        tool_state.move_project(1);
        assert_eq!(tool_state.cursor, 4);

        tool_state.move_project(1);
        assert_eq!(tool_state.cursor, 4);

        tool_state.move_project(-1);
        assert_eq!(tool_state.cursor, 2);

        tool_state.move_project(-1);
        assert_eq!(tool_state.cursor, 0);
    }

    #[test]
    fn toggles_all_sessions() {
        let mut tool_state = ToolState::loaded(
            Tool::Codex,
            vec![session_in("a-1", "a"), session_in("b-1", "b")],
        );

        tool_state.toggle_all_selected();

        assert_eq!(tool_state.selected.len(), 2);
        assert!(tool_state.selected.contains(&PathBuf::from("a/a-1.jsonl")));
        assert!(tool_state.selected.contains(&PathBuf::from("b/b-1.jsonl")));

        tool_state.toggle_all_selected();

        assert!(tool_state.selected.is_empty());
    }

    fn session(id: &str, path: PathBuf) -> SessionEntry {
        SessionEntry {
            tool: Tool::Codex,
            id: id.to_owned(),
            project: Some(String::from("project")),
            path,
            updated_at: None,
        }
    }

    fn session_in(id: &str, project: &str) -> SessionEntry {
        SessionEntry {
            tool: Tool::Codex,
            id: id.to_owned(),
            project: Some(project.to_owned()),
            path: PathBuf::from(project).join(format!("{id}.jsonl")),
            updated_at: None,
        }
    }
}
