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

use super::delete_selected_sessions;
use crate::DeleteOutcome;
use crate::model::session::{SessionEntry, Tool};
use crate::sources::SourceRegistry;

pub fn run_select_app(
    registry: &SourceRegistry,
    initial_tool: Option<Tool>,
    skip_confirmation: bool,
) -> Result<Option<(Tool, usize)>> {
    let mut app = SelectApp::new(registry, initial_tool, skip_confirmation)?;
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
            AppEvent::Quit => return Ok(None),
            AppEvent::Deleted(tool, deleted) => return Ok(Some((tool, deleted))),
        }
    }
}

struct SelectApp<'a> {
    registry: &'a SourceRegistry,
    tools: Vec<ToolState>,
    active_tool: usize,
    focus: Focus,
    pending_delete: bool,
    skip_confirmation: bool,
    session_page_size: usize,
    status: Option<String>,
}

impl<'a> SelectApp<'a> {
    fn new(
        registry: &'a SourceRegistry,
        initial_tool: Option<Tool>,
        skip_confirmation: bool,
    ) -> Result<Self> {
        let scoped_tool = initial_tool;
        let tools = initial_tool
            .map(|tool| vec![tool])
            .unwrap_or_else(|| Tool::all().to_vec())
            .into_iter()
            .map(ToolState::unloaded)
            .collect();
        let mut app = Self {
            registry,
            tools,
            active_tool: 0,
            focus: Focus::Tools,
            pending_delete: false,
            skip_confirmation,
            session_page_size: 1,
            status: None,
        };
        match app.load_active_tool() {
            Ok(()) => {
                if app.current_tool().sessions.is_empty() {
                    app.status = Some(String::from("no sessions found"));
                }
            }
            Err(error) => {
                if scoped_tool.is_some() {
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
            .constraints([Constraint::Min(1), Constraint::Length(4)])
            .areas(frame.area());
        let [tools, sessions] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(24), Constraint::Min(1)])
            .areas(body);

        self.session_page_size = usize::from(sessions.height.saturating_sub(2)).max(1);
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

        if let LoadState::Failed(error) = &tool_state.load_state {
            let error = Paragraph::new(error.as_str())
                .block(panel_block(&title, self.focus == Focus::Sessions));
            frame.render_widget(error, area);
            return;
        }

        if tool_state.sessions.is_empty() {
            let empty = Paragraph::new("no sessions")
                .block(panel_block(&title, self.focus == Focus::Sessions));
            frame.render_widget(empty, area);
            return;
        }

        let session_view = session_view(tool_state, usize::from(area.height.saturating_sub(2)));
        let list = List::new(session_view.items)
            .block(panel_block(&title, self.focus == Focus::Sessions))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        let mut state = ListState::default();
        state.select(session_view.selected_row);
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn render_footer(&self, frame: &mut Frame<'_>, area: Rect) {
        let tool_state = self.current_tool();
        let status = if self.pending_delete {
            format!(
                "press enter again to delete {} session(s), move or esc to cancel",
                tool_state.selected.len()
            )
        } else {
            self.status
                .clone()
                .unwrap_or_else(|| format!("{} selected", tool_state.selected.len()))
        };
        let help = Line::from(vec![
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
        ]);
        let footer = Paragraph::new(vec![Line::from(status), help])
            .block(Block::default().borders(Borders::ALL).title("status"));
        frame.render_widget(footer, area);
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<AppEvent> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Ok(AppEvent::Quit);
        }

        match key.code {
            KeyCode::Esc => self.pending_delete = false,
            KeyCode::Tab => {
                self.pending_delete = false;
                self.focus = self.focus.next();
            }
            KeyCode::Up => {
                self.pending_delete = false;
                match self.focus {
                    Focus::Tools => self.move_tool_cursor(-1),
                    Focus::Sessions => self.current_tool_mut().move_cursor(-1),
                }
            }
            KeyCode::Down => {
                self.pending_delete = false;
                match self.focus {
                    Focus::Tools => self.move_tool_cursor(1),
                    Focus::Sessions => self.current_tool_mut().move_cursor(1),
                }
            }
            KeyCode::Char('j') => {
                self.pending_delete = false;
                if self.focus == Focus::Sessions {
                    let page = self.session_page_size;
                    self.current_tool_mut().page_cursor(page as isize);
                }
            }
            KeyCode::Char('k') => {
                self.pending_delete = false;
                if self.focus == Focus::Sessions {
                    let page = self.session_page_size;
                    self.current_tool_mut().page_cursor(-(page as isize));
                }
            }
            KeyCode::Char(' ') => {
                self.pending_delete = false;
                if self.focus == Focus::Sessions {
                    self.current_tool_mut().toggle_selected();
                    self.status = None;
                }
            }
            KeyCode::Enter => return self.submit_current_tool(),
            _ => {}
        }

        Ok(AppEvent::Continue)
    }

    fn submit_current_tool(&mut self) -> Result<AppEvent> {
        let tool = self.current_tool().tool;
        let selected_paths = self.current_tool().selected.clone();
        if selected_paths.is_empty() {
            self.pending_delete = false;
            self.status = Some(String::from("select at least one session"));
            return Ok(AppEvent::Continue);
        }

        if !self.skip_confirmation && !self.pending_delete {
            self.pending_delete = true;
            return Ok(AppEvent::Continue);
        }

        let sessions = self.current_tool().sessions.clone();
        let outcome =
            delete_selected_sessions(self.registry.source(tool), &sessions, &selected_paths)?;
        self.pending_delete = false;

        match outcome {
            DeleteOutcome::Deleted(deleted) => {
                return Ok(AppEvent::Deleted(tool, deleted));
            }
            DeleteOutcome::NoSessionsFound => self.status = Some(String::from("no sessions found")),
            DeleteOutcome::NoSessionsDeleted => {
                self.status = Some(String::from("select at least one session"));
            }
        }

        Ok(AppEvent::Continue)
    }

    fn move_tool_cursor(&mut self, direction: isize) {
        let next = offset_index(self.active_tool, direction, self.tools.len());
        if next == self.active_tool {
            return;
        }

        self.active_tool = next;
        self.pending_delete = false;
        match self.load_active_tool() {
            Ok(()) => {
                self.status = self
                    .current_tool()
                    .sessions
                    .is_empty()
                    .then_some(String::from("no sessions found"));
            }
            Err(error) => self.status = Some(format!("{}: {error}", self.current_tool().tool)),
        }
    }

    fn current_tool(&self) -> &ToolState {
        &self.tools[self.active_tool]
    }

    fn current_tool_mut(&mut self) -> &mut ToolState {
        &mut self.tools[self.active_tool]
    }

    fn load_active_tool(&mut self) -> Result<()> {
        let active_tool = self.active_tool;
        let registry = self.registry;
        self.tools[active_tool].load(registry)
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

enum LoadState {
    Unloaded,
    Ready,
    Failed(String),
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

    fn load(&mut self, registry: &SourceRegistry) -> Result<()> {
        if matches!(self.load_state, LoadState::Ready) {
            return Ok(());
        }

        match registry.source(self.tool).list_sessions() {
            Ok(sessions) => {
                self.sessions = sessions;
                let row_cache = build_display_rows(&self.sessions);
                self.rows = row_cache.rows;
                self.session_rows = row_cache.session_rows;
                self.cursor = self.cursor.min(self.sessions.len().saturating_sub(1));
                self.cursor_row = self.session_rows.get(self.cursor).copied().unwrap_or(0);
                let current_paths = self
                    .sessions
                    .iter()
                    .map(|session| session.path.clone())
                    .collect::<BTreeSet<_>>();
                self.selected.retain(|path| current_paths.contains(path));
                self.load_state = LoadState::Ready;
                Ok(())
            }
            Err(error) => {
                self.sessions.clear();
                self.rows.clear();
                self.session_rows.clear();
                self.selected.clear();
                self.cursor = 0;
                self.cursor_row = 0;
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

    fn page_cursor(&mut self, direction: isize) {
        self.move_cursor(direction);
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
}

struct SessionListView {
    items: Vec<ListItem<'static>>,
    selected_row: Option<usize>,
}

struct RowCache {
    rows: Vec<DisplayRow>,
    session_rows: Vec<usize>,
}

enum DisplayRow {
    Header(String),
    Session { session_index: usize, text: String },
}

fn build_display_rows(sessions: &[SessionEntry]) -> RowCache {
    let mut rows = Vec::new();
    let mut session_rows = Vec::with_capacity(sessions.len());
    let mut current_project: Option<&str> = None;

    for (session_index, session) in sessions.iter().enumerate() {
        let project = session.project_name();
        if current_project != Some(project) {
            rows.push(DisplayRow::Header(format!("  [{project}]")));
            current_project = Some(project);
        }

        rows.push(DisplayRow::Session {
            session_index,
            text: session.display_line().to_owned(),
        });
        session_rows.push(rows.len().saturating_sub(1));
    }

    RowCache { rows, session_rows }
}

fn session_view(tool_state: &ToolState, visible_rows: usize) -> SessionListView {
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

    SessionListView {
        items,
        selected_row: Some(cursor_row - start),
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
