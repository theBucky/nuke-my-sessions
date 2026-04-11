mod paging;
mod rows;

use anyhow::Result;
use dialoguer::{
    console::{Key, Term, style},
    theme::Theme,
};

use crate::model::session::SessionEntry;

use self::paging::{Viewport, compute_pager, rendered_line_count, viewport_for_page};
use self::rows::{
    DisplayRow, build_display_rows, first_session_in_range, next_page_with_session,
    next_session_in_range, prev_page_with_session, prev_session_in_range,
};

pub(super) fn select_grouped_sessions(
    sessions: &[SessionEntry],
    theme: &dyn Theme,
) -> Result<Vec<usize>> {
    let rows = build_display_rows(sessions);
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let term = Term::stderr();
    let total_rows = rows.len();
    let mut checked = vec![false; sessions.len()];
    let mut cursor = rows.iter().position(DisplayRow::is_session).unwrap_or(0);
    let mut current_page = 0;
    let mut rendered_lines = 0;

    term.hide_cursor()?;
    let result = (|| -> Result<Vec<usize>> {
        loop {
            if rendered_lines > 0 {
                term.clear_last_lines(rendered_lines)?;
            }

            let pager = compute_pager(total_rows, term.size().0);
            current_page = current_page.min(pager.page_count.saturating_sub(1));
            let mut viewport = viewport_for_page(total_rows, pager, current_page);

            if cursor < viewport.start || cursor >= viewport.end || !rows[cursor].is_session() {
                if let Some(next_cursor) =
                    first_session_in_range(&rows, viewport.start, viewport.end)
                {
                    cursor = next_cursor;
                } else if let Some((next_page, next_cursor)) =
                    next_page_with_session(&rows, total_rows, pager, current_page, false)
                {
                    current_page = next_page;
                    viewport = viewport_for_page(total_rows, pager, current_page);
                    cursor = next_cursor;
                }
            }

            rendered_lines = render_rows(&term, &rows, &checked, cursor, viewport, theme)?;

            match term.read_key()? {
                Key::ArrowDown => {
                    if let Some(next_cursor) = next_session_in_range(&rows, cursor, viewport.end) {
                        cursor = next_cursor;
                        continue;
                    }

                    if let Some((next_page, next_cursor)) =
                        next_page_with_session(&rows, total_rows, pager, current_page, false)
                    {
                        current_page = next_page;
                        cursor = next_cursor;
                    }
                }
                Key::ArrowUp => {
                    if let Some(next_cursor) = prev_session_in_range(&rows, cursor, viewport.start)
                    {
                        cursor = next_cursor;
                        continue;
                    }

                    if let Some((next_page, next_cursor)) =
                        prev_page_with_session(&rows, total_rows, pager, current_page, true)
                    {
                        current_page = next_page;
                        cursor = next_cursor;
                    }
                }
                Key::ArrowRight => {
                    if let Some((next_page, next_cursor)) =
                        next_page_with_session(&rows, total_rows, pager, current_page, false)
                    {
                        current_page = next_page;
                        cursor = next_cursor;
                    }
                }
                Key::ArrowLeft => {
                    if let Some((next_page, next_cursor)) =
                        prev_page_with_session(&rows, total_rows, pager, current_page, false)
                    {
                        current_page = next_page;
                        cursor = next_cursor;
                    }
                }
                Key::Char(' ') => {
                    if let Some(index) = rows[cursor].session_index() {
                        checked[index] = !checked[index];
                    }
                }
                Key::Char('a') => {
                    let next_state = !checked.iter().all(|value| *value);
                    checked.fill(next_state);
                }
                Key::Enter => {
                    if rendered_lines > 0 {
                        term.clear_last_lines(rendered_lines)?;
                    }

                    return Ok(checked
                        .into_iter()
                        .enumerate()
                        .filter_map(|(index, selected)| selected.then_some(index))
                        .collect());
                }
                _ => {}
            }
        }
    })();

    let _ = term.show_cursor();
    result
}

fn render_rows(
    term: &Term,
    rows: &[DisplayRow],
    checked: &[bool],
    cursor: usize,
    viewport: Viewport,
    theme: &dyn Theme,
) -> Result<usize> {
    for (index, row) in rows
        .iter()
        .enumerate()
        .take(viewport.end)
        .skip(viewport.start)
    {
        match row {
            DisplayRow::Header(text) => term.write_line(&format!("  {}", style(text).dim()))?,
            DisplayRow::Session {
                session_index,
                text,
            } => term.write_line(&format_with_theme(|buffer| {
                theme.format_multi_select_prompt_item(
                    buffer,
                    text,
                    checked[*session_index],
                    index == cursor,
                )
            }))?,
        }
    }

    term.write_line(&format!(
        "{}",
        style(format!(
            "page: {}/{}",
            viewport.page + 1,
            viewport.pager.page_count
        ))
        .dim()
    ))?;
    term.write_line(&format!(
        "{}",
        style("new/prev: \u{2190}/\u{2192}, select/all: <space>/<a>").dim()
    ))?;

    term.flush()?;
    Ok(rendered_line_count(viewport))
}

fn format_with_theme(render: impl FnOnce(&mut String) -> std::fmt::Result) -> String {
    let mut buffer = String::new();
    let _ = render(&mut buffer);
    buffer
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::paging::Pager;
    use super::{
        build_display_rows, compute_pager, next_page_with_session, prev_page_with_session,
        rendered_line_count, viewport_for_page,
    };
    use crate::model::session::{SessionEntry, Tool};

    #[test]
    fn builds_grouped_rows() {
        let sessions = vec![
            session("a", Some("alpha")),
            session("b", Some("beta")),
            session("c", Some("beta")),
        ];

        let rows = build_display_rows(&sessions);
        assert_eq!(rows.len(), 5);
        assert!(matches!(rows[0], super::DisplayRow::Header(_)));
        assert!(matches!(rows[1], super::DisplayRow::Session { .. }));
    }

    #[test]
    fn computes_pager_with_footer_budget() {
        let pager = compute_pager(50, 10);
        assert_eq!(pager.capacity, 7);
        assert_eq!(pager.page_count, 8);

        let viewport = viewport_for_page(50, pager, 2);
        assert_eq!(viewport.start, 14);
        assert_eq!(viewport.end, 21);
        assert_eq!(rendered_line_count(viewport), 9);
    }

    #[test]
    fn clamps_single_page_viewport() {
        let pager = compute_pager(4, 10);
        assert_eq!(pager.capacity, 7);
        assert_eq!(pager.page_count, 1);

        let viewport = viewport_for_page(4, pager, 99);
        assert_eq!(viewport.page, 0);
        assert_eq!(viewport.start, 0);
        assert_eq!(viewport.end, 4);
    }

    #[test]
    fn finds_next_and_previous_pages_with_sessions() {
        let rows = build_display_rows(&[
            session("a", Some("alpha")),
            session("b", Some("beta")),
            session("c", Some("beta")),
            session("d", Some("gamma")),
        ]);
        let pager = Pager {
            capacity: 2,
            page_count: rows.len().div_ceil(2),
        };

        assert_eq!(
            next_page_with_session(&rows, rows.len(), pager, 0, false),
            Some((1, 3))
        );
        assert_eq!(
            prev_page_with_session(&rows, rows.len(), pager, 2, true),
            Some((1, 3))
        );
    }

    fn session(id: &str, project: Option<&str>) -> SessionEntry {
        SessionEntry {
            tool: Tool::Codex,
            id: id.into(),
            project: project.map(ToOwned::to_owned),
            path: PathBuf::from(format!("{id}.jsonl")),
            updated_at: None,
        }
    }
}
