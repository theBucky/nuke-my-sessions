use std::borrow::Cow;
use std::collections::BTreeSet;
use std::io::{self, Stdout};
use std::path::PathBuf;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use super::{ScopedSelection, delete_selected_sessions};
use crate::DeleteOutcome;
use crate::model::session::{SessionEntry, Tool, for_each_project_group};
use crate::sources::SourceRegistry;

const TOOLS_PANEL_WIDTH: u16 = 24;
const FOOTER_HEIGHT: u16 = 4;
const NO_SESSIONS_STATUS: &str = "no sessions found";
const EMPTY_SELECTION_STATUS: &str = "select at least one session";

pub fn run_select_app(
    registry: &SourceRegistry,
    scoped: Option<ScopedSelection>,
    skip_confirmation: bool,
) -> Result<SelectFlowOutcome> {
    let mut app = SelectApp::new(registry, scoped, skip_confirmation)?;

    let mut terminal = TerminalGuard::new()?;

    loop {
        terminal.draw(|frame| app.render(frame))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match app.handle_key(key)? {
            AppEvent::Continue => {}
            AppEvent::Quit => return Ok(SelectFlowOutcome::Cancelled),
            AppEvent::Deleted(tool, deleted) => {
                return Ok(SelectFlowOutcome::Deleted(tool, deleted));
            }
        }
    }
}

pub(crate) enum SelectFlowOutcome {
    Cancelled,
    Deleted(Tool, usize),
}

struct SelectApp<'a> {
    registry: &'a SourceRegistry,
    tools: Vec<ToolState>,
    active_tool: usize,
    focus: Focus,
    pending_delete: bool,
    skip_confirmation: bool,
    session_page_size: i16,
    status: Option<String>,
}

impl<'a> SelectApp<'a> {
    fn new(
        registry: &'a SourceRegistry,
        scoped: Option<ScopedSelection>,
        skip_confirmation: bool,
    ) -> Result<Self> {
        let is_scoped = scoped.is_some();
        let tools = match scoped {
            Some(ScopedSelection { tool, sessions }) => vec![ToolState::loaded(tool, sessions)],
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

    fn render(&mut self, frame: &mut Frame<'_>) {
        let [body, footer] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(FOOTER_HEIGHT)])
            .areas(frame.area());
        let [tools, sessions] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(TOOLS_PANEL_WIDTH), Constraint::Min(1)])
            .areas(body);

        self.session_page_size =
            i16::try_from(sessions.height.saturating_sub(2).max(1)).unwrap_or(i16::MAX);
        self.render_tools(frame, tools);
        self.render_sessions(frame, sessions);
        self.render_footer(frame, footer);
    }

    fn render_tools(&self, frame: &mut Frame<'_>, area: Rect) {
        let items = self
            .tools
            .iter()
            .map(|state| ListItem::new(format!("{} ({})", state.tool, state.session_badge())));
        let list = List::new(items)
            .block(panel_block("tools", self.focus == Focus::Tools))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        let mut state = ListState::default();
        state.select(Some(self.active_tool));
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn render_sessions(&self, frame: &mut Frame<'_>, area: Rect) {
        let tool_state = self.current_tool();
        let title = format!("sessions: {}", tool_state.tool);
        let block = panel_block(&title, self.focus == Focus::Sessions);

        match &tool_state.load_state {
            LoadState::Failed(error) => {
                frame.render_widget(Paragraph::new(error.as_str()).block(block), area);
            }
            LoadState::Ready | LoadState::Unloaded if tool_state.sessions.is_empty() => {
                frame.render_widget(Paragraph::new("no sessions").block(block), area);
            }
            LoadState::Ready | LoadState::Unloaded => {
                let session_view =
                    SessionListView::new(tool_state, usize::from(area.height.saturating_sub(2)));
                let list = List::new(session_view.items)
                    .block(block)
                    .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
                let mut state = ListState::default();
                state.select(session_view.selected_row);
                frame.render_stateful_widget(list, area, &mut state);
            }
        }
    }

    fn render_footer(&self, frame: &mut Frame<'_>, area: Rect) {
        let footer = Paragraph::new(vec![Line::from(self.status_line()), Self::help_line()])
            .block(Block::default().borders(Borders::ALL).title("status"));
        frame.render_widget(footer, area);
    }

    fn status_line(&self) -> Cow<'_, str> {
        if self.pending_delete {
            return Cow::Owned(format!(
                "press enter again to delete {} session(s), move or esc to cancel",
                self.current_tool().selected.len()
            ));
        }

        self.status.as_deref().map_or_else(
            || Cow::Owned(format!("{} selected", self.current_tool().selected.len())),
            Cow::Borrowed,
        )
    }

    fn help_line() -> Line<'static> {
        Line::from(vec![
            hotkey("up/down"),
            plain(": move  "),
            hotkey("tab"),
            plain(": switch panel  "),
            hotkey("space"),
            plain(": toggle  "),
            hotkey("j/k"),
            plain(": page  "),
            hotkey("enter"),
            plain(": submit  "),
            hotkey("ctrl+c"),
            plain(": quit"),
        ])
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<AppEvent> {
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
            return self.submit_current_tool();
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

    fn submit_current_tool(&mut self) -> Result<AppEvent> {
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
            delete_selected_sessions(
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

#[derive(Clone, Copy, Eq, PartialEq)]
enum Focus {
    Tools,
    Sessions,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::Tools => Self::Sessions,
            Self::Sessions => Self::Tools,
        }
    }
}

enum AppEvent {
    Continue,
    Quit,
    Deleted(Tool, usize),
}

struct ToolState {
    tool: Tool,
    sessions: Vec<SessionEntry>,
    rows: Vec<DisplayRow>,
    session_rows: Vec<usize>,
    selected: BTreeSet<PathBuf>,
    cursor: usize,
    cursor_row: usize,
    load_state: LoadState,
}

impl ToolState {
    fn unloaded(tool: Tool) -> Self {
        Self {
            tool,
            sessions: Vec::new(),
            rows: Vec::new(),
            session_rows: Vec::new(),
            selected: BTreeSet::new(),
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

    fn session_badge(&self) -> String {
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
        let current_paths = self
            .sessions
            .iter()
            .map(|session| session.path.clone())
            .collect::<BTreeSet<_>>();
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

enum LoadState {
    Unloaded,
    Ready,
    Failed(String),
}

struct RowCache {
    rows: Vec<DisplayRow>,
    session_rows: Vec<usize>,
}

impl RowCache {
    fn build(sessions: &[SessionEntry]) -> Self {
        let mut rows = Vec::new();
        let mut session_rows = Vec::with_capacity(sessions.len());
        let mut session_index = 0;

        for_each_project_group(sessions, |project, project_sessions| {
            rows.push(DisplayRow::Header(format!("  [{project}]")));
            for session in project_sessions {
                rows.push(DisplayRow::Session {
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

enum DisplayRow {
    Header(String),
    Session { session_index: usize, text: String },
}

struct SessionListView {
    items: Vec<ListItem<'static>>,
    selected_row: Option<usize>,
}

impl SessionListView {
    fn new(tool_state: &ToolState, visible_rows: usize) -> Self {
        let visible_rows = visible_rows.max(1);
        let cursor_row = tool_state.cursor_row;
        let max_start = tool_state.rows.len().saturating_sub(visible_rows);
        let start = cursor_row.saturating_sub(visible_rows / 2).min(max_start);
        let end = (start + visible_rows).min(tool_state.rows.len());

        let items = tool_state.rows[start..end]
            .iter()
            .map(|row| match row {
                DisplayRow::Header(text) => ListItem::new(Line::from(vec![Span::styled(
                    text.clone(),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )])),
                DisplayRow::Session {
                    session_index,
                    text,
                } => {
                    let marker = if tool_state
                        .selected
                        .contains(&tool_state.sessions[*session_index].path)
                    {
                        "[x]"
                    } else {
                        "[ ]"
                    };

                    ListItem::new(format!(" {marker} {text}"))
                }
            })
            .collect();

        Self {
            items,
            selected_row: Some(cursor_row - start),
        }
    }
}

fn panel_block(title: &str, focused: bool) -> Block<'_> {
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style)
}

fn hotkey(text: &'static str) -> Span<'static> {
    Span::styled(
        text,
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
}

fn plain(text: &'static str) -> Span<'static> {
    Span::raw(text)
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

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        let result = (|| -> Result<Self> {
            let mut stdout = io::stdout();
            execute!(stdout, EnterAlternateScreen)?;
            let backend = CrosstermBackend::new(stdout);
            let terminal = Terminal::new(backend)?;

            Ok(Self { terminal })
        })();
        if result.is_err() {
            let _ = disable_raw_mode();
            let mut stdout = io::stdout();
            let _ = execute!(stdout, LeaveAlternateScreen);
        }

        result
    }

    fn draw(&mut self, render: impl FnOnce(&mut Frame<'_>)) -> Result<()> {
        self.terminal.draw(render)?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}
