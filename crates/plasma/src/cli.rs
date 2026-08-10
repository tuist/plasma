use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

use plasma_openrouter::{import_pi_openrouter_credential, save_key};

#[derive(Parser)]
#[command(name = "plasma", about = "A coding agent")]
pub struct Cli {
    /// Submit a prompt without starting the terminal interface.
    #[arg(short = 'p', long, global = true)]
    pub prompt: Option<String>,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Copy the OpenRouter credential from Pi without printing it.
    ImportPiCredentials,
    /// Run one coding task without an interactive terminal interface.
    Exec {
        /// Task for Plasma to complete. When omitted, Plasma reads the task from standard input.
        #[arg(value_name = "PROMPT")]
        prompt: Option<String>,
        /// OpenRouter credential. Prefer PLASMA_OPENROUTER_API_KEY for scripts.
        #[arg(long)]
        api_key: Option<String>,
        /// Directory in which tools run. Defaults to the current directory.
        #[arg(long, value_name = "PATH")]
        cwd: Option<std::path::PathBuf>,
        /// Include tool activity on standard error.
        #[arg(short, long)]
        verbose: bool,
        /// Print the final response as machine-readable data.
        #[arg(long)]
        json: bool,
        /// Control terminal color in the final response.
        #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
        color: ColorChoice,
    },
    Connect {
        provider: String,
        #[arg(long)]
        api_key: String,
    },
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Some(Command::ImportPiCredentials) => {
            if import_pi_openrouter_credential()? {
                println!("Imported OpenRouter credential from Pi.");
            } else {
                println!("Plasma already has an OpenRouter credential.");
            }
            Ok(())
        }
        Some(Command::Exec {
            prompt,
            api_key,
            cwd,
            verbose,
            json,
            color,
        }) => crate::headless::run(prompt.or(cli.prompt), api_key, cwd, verbose, json, color),
        Some(Command::Connect { provider, api_key }) => {
            anyhow::ensure!(
                provider.eq_ignore_ascii_case("openrouter"),
                "only OpenRouter is supported currently"
            );
            save_key(&api_key)?;
            println!("OpenRouter is connected.");
            Ok(())
        }
        None if cli.prompt.is_some() => {
            crate::headless::run(cli.prompt, None, None, false, false, ColorChoice::Auto)
        }
        None => crate::ui::run(),
    }
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_accepts_a_positional_prompt() {
        let cli = Cli::try_parse_from(["plasma", "exec", "inspect the workspace"]).unwrap();
        let Some(Command::Exec { prompt, .. }) = cli.command else {
            panic!("expected exec command");
        };
        assert_eq!(prompt.as_deref(), Some("inspect the workspace"));
    }

    #[test]
    fn short_prompt_option_starts_headless_mode() {
        let cli = Cli::try_parse_from(["plasma", "-p", "inspect the workspace"]).unwrap();
        assert_eq!(cli.prompt.as_deref(), Some("inspect the workspace"));
        assert!(cli.command.is_none());
    }
}
