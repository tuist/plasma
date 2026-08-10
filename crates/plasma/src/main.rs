use anyhow::Result;
use clap::Parser;

mod agent_prompt;
mod app;
mod browser;
mod cli;
mod footer;
mod headless;
mod input;
mod oauth;
mod openrouter_oauth;
mod rich_text;
mod syntax;
mod theme;
mod ui;

fn main() -> Result<()> {
    cli::run(Cli::parse())
}

use cli::Cli;
