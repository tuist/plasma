use anyhow::Result;
use clap::{Parser, Subcommand};

use plasma_openrouter::save_key;

#[derive(Parser)]
#[command(name = "plasma", about = "A coding agent")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run as a headless Agent Client Protocol server over standard input/output.
    Acp,
    Connect {
        provider: String,
        #[arg(long)]
        api_key: String,
    },
}

pub fn run(cli: Cli) -> Result<()> {
    if matches!(cli.command, Some(Command::Acp)) {
        tokio::runtime::Runtime::new()?.block_on(crate::acp::run())?;
        return Ok(());
    }
    if let Some(Command::Connect { provider, api_key }) = cli.command {
        anyhow::ensure!(
            provider.eq_ignore_ascii_case("openrouter"),
            "only OpenRouter is supported currently"
        );
        save_key(&api_key)?;
        println!("OpenRouter is connected.");
        return Ok(());
    }
    crate::ui::run()
}
