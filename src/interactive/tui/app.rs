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
    ) -> Result<Self> {
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
            session_page_size: 1,
            status: None,
        };

        match app.load_active_tool() {
            Ok(()) => app.sync_status_with_active_tool(),
            Err(error) => {
                if is_scoped {
                    return Err(error);
                }
                app.status = Some(format!("{}: {error}", app.current_tool().tool));
            }
        }

        Ok(app)
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
            KeyCode::Char('j') => self.move_session_page(1),
            KeyCode::Char('k') => self.move_session_page(-1),
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

    fn move_session_page(&mut self, page_direction: isize) {
        if self.focus != Focus::Sessions {
            return;
        }

        self.clear_pending_delete();
        let offset = isize::from(self.session_page_size) * page_direction;
        self.current_tool_mut().move_cursor(offset);
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
            DeleteOutcome::Deleted(deleted) => Ok(AppEvent::Deleted(tool, deleted)),
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
            Err(error) => {
                self.status = Some(format!("{}: {error}", self.current_tool().tool));
            }
        }
    }

    fn sync_status_with_active_tool(&mut self) {
        self.status = self
            .current_tool()
            .sessions
            .is_empty()
            .then_some(String::from(NO_SESSIONS_STATUS));
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
            selected: Default::default(),
            cursor: 0,
            cursor_row: 0,
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
                self.reset();
                self.load_state = LoadState::Failed(error.to_string());
                Err(error)
            }
        }
    }

    fn move_cursor(&mut self, direction: isize) {
        if self.sessions.is_empty() {
            self.cursor = 0;
            return;
        }

        self.cursor = offset_index(self.cursor, direction, self.sessions.len());
        self.cursor_row = self.session_rows.get(self.cursor).copied().unwrap_or(0);
    }

    fn toggle_selected(&mut self) {
        let Some(session) = self.sessions.get(self.cursor) else {
            return;
        };

        if !self.selected.insert(session.path.clone()) {
            self.selected.remove(&session.path);
        }
    }

    pub(super) fn session_badge(&self) -> String {
        match &self.load_state {
            LoadState::Failed(_) => String::from("!"),
            LoadState::Ready => self.sessions.len().to_string(),
            LoadState::Unloaded => String::from("-"),
        }
    }

    fn apply_row_cache(&mut self) {
        let row_cache = RowCache::build(&self.sessions);
        self.rows = row_cache.rows;
        self.session_rows = row_cache.session_rows;
        self.cursor = self.cursor.min(self.sessions.len().saturating_sub(1));
        self.cursor_row = self.session_rows.get(self.cursor).copied().unwrap_or(0);
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
        self.sessions = sessions;
        self.apply_row_cache();
        self.retain_existing_selection();
        self.load_state = LoadState::Ready;
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
