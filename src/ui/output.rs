use crate::DeleteOutcome;
use crate::model::session::{SessionEntry, Tool, for_each_project_group};

pub fn print_tool_header(tool: Tool) {
    println!("{tool}:");
}

pub fn print_sessions(sessions: &[SessionEntry]) {
    if sessions.is_empty() {
        println!("  no sessions");
        return;
    }

    for_each_project_group(sessions, |project, sessions| {
        println!("  [{project}]");
        for session in sessions {
            println!("    {}", session.display_line());
        }
    });
}

pub fn print_delete_outcome(tool: Tool, outcome: DeleteOutcome) {
    match outcome {
        DeleteOutcome::NoSessionsFound => println!("{tool}: no sessions found"),
        DeleteOutcome::NoSessionsDeleted => println!("{tool}: no sessions deleted"),
        DeleteOutcome::Deleted(deleted_count) => {
            println!("{tool}: deleted {deleted_count} session(s)");
        }
    }
}
