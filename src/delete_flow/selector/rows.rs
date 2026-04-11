use crate::model::session::SessionEntry;

use super::paging::{Pager, viewport_for_page};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum DisplayRow {
    Header(String),
    Session { session_index: usize, text: String },
}

impl DisplayRow {
    pub(super) fn is_session(&self) -> bool {
        matches!(self, Self::Session { .. })
    }

    pub(super) fn session_index(&self) -> Option<usize> {
        match self {
            Self::Session { session_index, .. } => Some(*session_index),
            Self::Header(_) => None,
        }
    }
}

pub(super) fn build_display_rows(sessions: &[SessionEntry]) -> Vec<DisplayRow> {
    let mut rows = Vec::new();
    let mut current_project: Option<&str> = None;

    for (index, session) in sessions.iter().enumerate() {
        let project = session.project_name();
        if current_project != Some(project) {
            rows.push(DisplayRow::Header(format!("--- {project} ---")));
            current_project = Some(project);
        }

        rows.push(DisplayRow::Session {
            session_index: index,
            text: session.display_line().to_owned(),
        });
    }

    rows
}

pub(super) fn first_session_in_range(
    rows: &[DisplayRow],
    start: usize,
    end: usize,
) -> Option<usize> {
    (start..end).find(|index| rows[*index].is_session())
}

pub(super) fn next_session_in_range(
    rows: &[DisplayRow],
    cursor: usize,
    end: usize,
) -> Option<usize> {
    ((cursor + 1)..end).find(|index| rows[*index].is_session())
}

pub(super) fn prev_session_in_range(
    rows: &[DisplayRow],
    cursor: usize,
    start: usize,
) -> Option<usize> {
    (start..cursor)
        .rev()
        .find(|index| rows[*index].is_session())
}

pub(super) fn next_page_with_session(
    rows: &[DisplayRow],
    total_rows: usize,
    pager: Pager,
    current_page: usize,
    prefer_last: bool,
) -> Option<(usize, usize)> {
    find_page_with_session(
        rows,
        total_rows,
        pager,
        current_page.saturating_add(1),
        true,
        prefer_last,
    )
}

pub(super) fn prev_page_with_session(
    rows: &[DisplayRow],
    total_rows: usize,
    pager: Pager,
    current_page: usize,
    prefer_last: bool,
) -> Option<(usize, usize)> {
    if current_page == 0 {
        return None;
    }

    find_page_with_session(
        rows,
        total_rows,
        pager,
        current_page - 1,
        false,
        prefer_last,
    )
}

fn find_page_with_session(
    rows: &[DisplayRow],
    total_rows: usize,
    pager: Pager,
    start_page: usize,
    forward: bool,
    prefer_last: bool,
) -> Option<(usize, usize)> {
    if forward {
        for page in start_page..pager.page_count {
            let viewport = viewport_for_page(total_rows, pager, page);
            let cursor = session_in_range(rows, viewport.start, viewport.end, prefer_last);
            if let Some(cursor) = cursor {
                return Some((page, cursor));
            }
        }

        return None;
    }

    for page in (0..=start_page.min(pager.page_count.saturating_sub(1))).rev() {
        let viewport = viewport_for_page(total_rows, pager, page);
        let cursor = session_in_range(rows, viewport.start, viewport.end, prefer_last);
        if let Some(cursor) = cursor {
            return Some((page, cursor));
        }
    }

    None
}

fn session_in_range(
    rows: &[DisplayRow],
    start: usize,
    end: usize,
    prefer_last: bool,
) -> Option<usize> {
    if prefer_last {
        return (start..end).rev().find(|index| rows[*index].is_session());
    }

    (start..end).find(|index| rows[*index].is_session())
}
