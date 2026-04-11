const FOOTER_LINES: usize = 2;
const SAFETY_LINES: usize = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Pager {
    pub(super) capacity: usize,
    pub(super) page_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Viewport {
    pub(super) page: usize,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) pager: Pager,
}

pub(super) fn compute_pager(total_rows: usize, term_rows: u16) -> Pager {
    let available_lines = usize::from(term_rows).saturating_sub(SAFETY_LINES).max(1);
    let capacity = available_lines.saturating_sub(FOOTER_LINES).max(1);

    Pager {
        capacity,
        page_count: total_rows.div_ceil(capacity).max(1),
    }
}

pub(super) fn viewport_for_page(total_rows: usize, pager: Pager, page: usize) -> Viewport {
    let page = page.min(pager.page_count.saturating_sub(1));
    let start = page * pager.capacity;
    let end = (start + pager.capacity).min(total_rows);

    Viewport {
        page,
        start,
        end,
        pager,
    }
}

pub(super) fn rendered_line_count(viewport: Viewport) -> usize {
    (viewport.end - viewport.start) + FOOTER_LINES
}
