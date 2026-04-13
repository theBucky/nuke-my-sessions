mod delete_flow;
mod model;
mod sources;
mod ui;

use std::io::{IsTerminal, stdin, stdout};

use anyhow::{Result, bail};
use clap::Parser;

use crate::delete_flow::{DialoguerPrompter, run_select_app};
use crate::model::session::Tool;
use crate::sources::SourceRegistry;
use crate::ui::cli::{Cli, Command};
use crate::ui::output::{print_delete_outcome, print_sessions, print_tool_header};

pub(crate) enum DeleteOutcome {
    NoSessionsFound,
    NoSessionsDeleted,
    Deleted(usize),
}

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
    if let Some(tool) = tool {
        let sessions = registry.source(tool).list_sessions()?;
        if sessions.is_empty() {
            print_delete_outcome(tool, DeleteOutcome::NoSessionsFound);
            return Ok(());
        }
    }
    ensure_terminal()?;

    if let Some((tool, deleted)) = run_select_app(registry, tool, skip_confirmation)? {
        print_delete_outcome(tool, DeleteOutcome::Deleted(deleted));
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
            DialoguerPrompter::default().choose_tool()
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
        let mut prompter = DialoguerPrompter::default();
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
