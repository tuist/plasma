use anyhow::Result;
use clap::Parser;

mod acp;
mod app;
mod browser;
mod cli;
mod footer;
mod input;
mod oauth;
mod openrouter_oauth;
mod rich_text;
mod theme;
mod ui;

fn main() -> Result<()> {
    cli::run(Cli::parse())
}

use cli::Cli;
