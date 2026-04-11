use crate::session::{SessionEntry, Tool};

pub fn print_tool_header(tool: Tool) {
    println!("{tool}:");
}

pub fn print_sessions(sessions: &[SessionEntry]) {
    if sessions.is_empty() {
        println!("  no sessions");
        return;
    }

    for session in sessions {
        println!("  {}", session.display_line());
    }
}

pub fn print_delete_outcome(tool: Tool, deleted_count: usize) {
    if deleted_count == 0 {
        println!("{tool}: no sessions deleted");
        return;
    }

    println!("{tool}: deleted {deleted_count} session(s)");
}
