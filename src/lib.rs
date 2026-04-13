mod interactive;
mod model;
mod sources;
mod ui;

use std::io::{IsTerminal, stdin, stdout};

use anyhow::{Result, bail};
use clap::Parser;

use crate::interactive::{InteractiveOutcome, Prompter, ToolSessions, run_session_browser};
use crate::model::session::Tool;
use crate::sources::SourceRegistry;
use crate::ui::cli::{Cli, Command};
use crate::ui::output::{print_delete_outcome, print_sessions, print_tool_header};

#[derive(Clone, Copy)]
pub(crate) enum DeleteOutcome {
    NoSessionsFound,
    NoSessionsDeleted,
    Deleted(usize),
}

/// Runs the CLI.
///
/// # Errors
///
/// Returns an error when argument parsing, session discovery, interactive terminal setup,
/// confirmation prompts, or session deletion fails.
pub fn run() -> Result<()> {
    let cli = Cli::parse();

    let registry = SourceRegistry::new()?;
    match cli.command.unwrap_or(Command::Select) {
        Command::List => list_sessions(&registry, cli.tool),
        Command::Select => select_sessions(&registry, cli.tool, cli.yes),
        Command::Nuke => nuke_sessions(&registry, cli.tool, cli.all, cli.yes),
    }
}

fn list_sessions(registry: &SourceRegistry, tool: Option<Tool>) -> Result<()> {
    let tools = tool.map_or_else(|| Tool::all().to_vec(), |tool| vec![tool]);

    for (index, tool) in tools.iter().enumerate() {
        if index > 0 {
            println!();
        }

        let sessions = registry.source(*tool).list_sessions()?;
        print_tool_header(*tool);
        print_sessions(&sessions);
    }

    Ok(())
}

fn select_sessions(
    registry: &SourceRegistry,
    tool: Option<Tool>,
    skip_confirmation: bool,
) -> Result<()> {
    let tool_sessions = if let Some(tool) = tool {
        let sessions = registry.source(tool).list_sessions()?;
        if sessions.is_empty() {
            print_delete_outcome(tool, DeleteOutcome::NoSessionsFound);
            return Ok(());
        }
        ensure_terminal()?;
        Some(ToolSessions { tool, sessions })
    } else {
        ensure_terminal()?;
        None
    };

    match run_session_browser(registry, tool_sessions, skip_confirmation)? {
        InteractiveOutcome::Cancelled => {}
        InteractiveOutcome::Deleted(tool, deleted) => {
            print_delete_outcome(tool, DeleteOutcome::Deleted(deleted));
        }
    }

    Ok(())
}

fn nuke_sessions(
    registry: &SourceRegistry,
    tool: Option<Tool>,
    delete_all: bool,
    skip_confirmation: bool,
) -> Result<()> {
    if !delete_all {
        bail!("`nuke` requires `--all`; use `select` for targeted deletion");
    }

    let tool = match tool {
        Some(tool) => tool,
        None if stdin().is_terminal() && stdout().is_terminal() => {
            Prompter::default().choose_tool()
        }
        None => bail!("`nuke --all` requires `--tool` when no interactive terminal is attached"),
    };

    let sessions = registry.source(tool).list_sessions()?;
    if sessions.is_empty() {
        print_delete_outcome(tool, DeleteOutcome::NoSessionsFound);
        return Ok(());
    }

    if !skip_confirmation {
        ensure_terminal()?;
        let prompter = Prompter::default();
        if !prompter.confirm_nuke_all(tool, sessions.len())? {
            print_delete_outcome(tool, DeleteOutcome::NoSessionsDeleted);
            return Ok(());
        }
    }

    let deleted = registry.source(tool).delete_sessions(&sessions)?.finish()?;
    print_delete_outcome(tool, DeleteOutcome::Deleted(deleted));

    Ok(())
}

fn ensure_terminal() -> Result<()> {
    if stdin().is_terminal() && stdout().is_terminal() {
        return Ok(());
    }

    bail!("interactive mode requires a terminal")
}
