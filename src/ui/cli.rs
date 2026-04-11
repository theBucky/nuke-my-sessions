use clap::{Parser, Subcommand};

use crate::model::session::Tool;

#[derive(Debug, Parser)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(long, value_enum, global = true)]
    pub tool: Option<Tool>,

    #[arg(long, global = true)]
    pub all: bool,

    #[arg(long, short = 'y', global = true)]
    pub yes: bool,

    #[arg(long, global = true)]
    pub verbose: bool,
}

#[derive(Clone, Copy, Debug, Subcommand)]
pub enum Command {
    List,
    Select,
    Nuke,
}
