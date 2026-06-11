use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "hivemind", version, about = "HiveMind — persistent memory for AI coding agents")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Scaffold .hivemind.toml + CLAUDE.md integration for this project
    Init,
    /// Show config and preview what session start will inject
    Status,
}

pub fn cmd_init() -> Result<()> {
    println!("init: not yet implemented");
    Ok(())
}

pub fn cmd_status() -> Result<()> {
    println!("status: not yet implemented");
    Ok(())
}
