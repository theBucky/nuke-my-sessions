use std::borrow::Cow;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph},
};

use super::{
    DisplayRow, FOOTER_HEIGHT, Focus, LoadState, PANEL_PADDING_X, SessionBrowser, SessionListView,
    TOOLS_PANEL_WIDTH,
};

impl SessionBrowser<'_> {
    pub(super) fn render(&mut self, frame: &mut Frame<'_>) {
        let [body, footer] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(FOOTER_HEIGHT)])
            .areas(frame.area());
        let [tools, sessions] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(TOOLS_PANEL_WIDTH), Constraint::Min(1)])
            .areas(body);

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
        let tool_state = &self.tools[self.active_tool];
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
                    SessionListView::new(tool_state, usize::from(block.inner(area).height));
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
            .block(titled_block("status"));
        frame.render_widget(footer, area);
    }

    fn status_line(&self) -> Cow<'_, str> {
        let tool_state = &self.tools[self.active_tool];
        if self.pending_delete {
            return Cow::Owned(format!(
                "press enter again to delete {} session(s), move or esc to cancel",
                tool_state.selected.len()
            ));
        }

        self.status.as_deref().map_or_else(
            || Cow::Owned(format!("{} selected", tool_state.selected.len())),
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
            plain(": project  "),
            hotkey("a"),
            plain(": all/none  "),
            hotkey("enter"),
            plain(": submit  "),
            hotkey("ctrl+c"),
            plain(": quit"),
        ])
    }
}

impl SessionListView {
    fn new(tool_state: &super::ToolState, visible_rows: usize) -> Self {
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

    titled_block(title).border_style(border_style)
}

fn titled_block(title: &str) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .padding(Padding::horizontal(PANEL_PADDING_X))
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
