mod app;
mod render;
mod terminal;

use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};

use crate::model::session::{SessionEntry, Tool};
use crate::sources::SourceRegistry;

use self::terminal::TerminalGuard;

const TOOLS_PANEL_WIDTH: u16 = 24;
const FOOTER_HEIGHT: u16 = 4;
const PANEL_PADDING_X: u16 = 1;
const NO_SESSIONS_STATUS: &str = "no sessions found";
const EMPTY_SELECTION_STATUS: &str = "select at least one session";

pub fn run_session_browser(
    registry: &SourceRegistry,
    tool_sessions: Option<super::ToolSessions>,
    skip_confirmation: bool,
) -> Result<()> {
    let mut browser = SessionBrowser::new(registry, tool_sessions, skip_confirmation);
    let mut terminal = TerminalGuard::new()?;

    loop {
        terminal.draw(|frame| browser.render(frame))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match browser.handle_key(key)? {
            AppEvent::Continue => {}
            AppEvent::Quit => return Ok(()),
        }
    }
}

struct SessionBrowser<'a> {
    registry: &'a SourceRegistry,
    tools: Vec<ToolState>,
    active_tool: usize,
    focus: Focus,
    pending_delete: bool,
    skip_confirmation: bool,
    status: Option<String>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Focus {
    Tools,
    Sessions,
}

enum AppEvent {
    Continue,
    Quit,
}

struct ToolState {
    tool: Tool,
    sessions: Vec<SessionEntry>,
    rows: Vec<DisplayRow>,
    session_rows: Vec<usize>,
    selected: BTreeSet<PathBuf>,
    cursor: usize,
    cursor_row: usize,
    session_count: Option<usize>,
    count_failed: bool,
    load_state: LoadState,
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

enum DisplayRow {
    Header(String),
    Session { session_index: usize, text: String },
}

struct SessionListView {
    items: Vec<ratatui::widgets::ListItem<'static>>,
    selected_row: Option<usize>,
}
