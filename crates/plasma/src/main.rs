use anyhow::Result;
use clap::Parser;

mod app;
mod browser;
mod cli;
mod input;
mod theme;
mod ui;

fn main() -> Result<()> {
    cli::run(Cli::parse())
}

use cli::Cli;
