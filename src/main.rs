//! CLI application for farming
//! brings `farmer` and `node` together

#![deny(missing_docs, clippy::unwrap_used)]
#![feature(concat_idents)]
#![feature(is_some_and)]

mod commands;
mod config;
mod summary;
mod utils;

#[cfg(test)]
mod tests;

use std::io::{self, Write};

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Context, Report};
use color_eyre::Help;
use owo_colors::OwoColorize;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use termion::cursor::{self, DetectCursorPos};
use termion::event::Key;
use termion::input::TermRead;
use termion::raw::IntoRawMode;
use tracing::instrument;

use crate::commands::farm::farm;
use crate::commands::info::info;
use crate::commands::init::init;
use crate::commands::wipe::wipe_config;
use crate::utils::{get_user_input, open_log_dir, support_message, yes_or_no_parser};

#[cfg(all(
    target_arch = "x86_64",
    target_vendor = "unknown",
    target_os = "linux",
    target_env = "gnu"
))]
#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[derive(Debug, Parser)]
#[command(subcommand_required = false)]
#[command(name = "subspace")]
#[command(about = "Subspace CLI", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

/// Available commands for the CLI
#[derive(Debug, Subcommand, EnumIter)]
enum Commands {
    #[command(about = "initializes the config file required for the farming")]
    Init,
    #[command(about = "starting the farming process (along with node in the background)")]
    Farm {
        #[arg(short, long, action)]
        verbose: bool,
        #[arg(short, long, action)]
        executor: bool,
        #[arg(short, long, action)]
        debug: bool,
    },
    #[command(about = "wipes the node and farm instance (along with your plots)")]
    Wipe {
        #[arg(long, action)]
        farmer: bool,
        #[arg(long, action)]
        node: bool,
    },
    #[command(about = "displays info about the farmer instance (i.e. total amount of rewards, \
                       and status of initial plotting)")]
    Info,
    OpenLogs,
}

#[tokio::main]
#[instrument]
async fn main() -> Result<(), Report> {
    let args = Cli::parse();
    match args.command {
        Some(Commands::Info) => {
            info().await.suggestion(support_message())?;
        }
        Some(Commands::Init) => {
            init().suggestion(support_message())?;
        }
        Some(Commands::Farm { verbose, executor, debug }) => {
            farm(verbose, executor, debug).await.suggestion(support_message())?;
        }
        Some(Commands::Wipe { farmer, node }) => {
            wipe_config(farmer, node).await.suggestion(support_message())?;
        }
        Some(Commands::OpenLogs) => {
            open_log_dir().suggestion(support_message())?;
        }
        None => {
            arrow_key_mode().await.suggestion(support_message())?;
        }
    }

    Ok(())
}

#[instrument]
async fn arrow_key_mode() -> Result<(), Report> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock().into_raw_mode()?;

    // Options to be displayed
    let options = Commands::iter().map(|command| command.to_string()).collect::<Vec<_>>();

    // Selected option index
    let mut selected = 0;

    // get the current location of the cursor
    let (_, y) = stdout.cursor_pos()?;

    // Print options to the terminal
    print_options(&mut stdout, &options, selected, y)?;

    // Process input events
    for c in io::stdin().keys() {
        match c.context("failed to read input")? {
            Key::Up | Key::Char('k') => {
                // Move selection up
                if selected > 0 {
                    selected -= 1;
                    print_options(&mut stdout, &options, selected, y)?;
                }
            }
            Key::Down | Key::Char('j') => {
                // Move selection down
                if selected < options.len() - 1 {
                    selected += 1;
                    print_options(&mut stdout, &options, selected, y)?;
                }
            }
            Key::Char('\n') => {
                stdout.suspend_raw_mode()?;
                break;
            }
            Key::Ctrl('c') => {
                return Ok(());
            }
            _ => {}
        }
    }

    stdout.suspend_raw_mode()?;

    match selected {
        0 => {
            init().suggestion(support_message())?;
        }
        1 => {
            let prompt = "Do you want to initialize farmer in verbose mode? [y/n]: ";
            let verbose =
                get_user_input(prompt, None, yes_or_no_parser).context("prompt failed")?;

            let prompt = "Do you want to be an executor? [y/n]: ";
            let executor =
                get_user_input(prompt, None, yes_or_no_parser).context("prompt failed")?;

            farm(verbose, executor, false).await.suggestion(support_message())?;
        }
        2 => {
            wipe_config(false, false).await.suggestion(support_message())?;
        }
        3 => {
            info().await.suggestion(support_message())?;
        }
        4 => {
            open_log_dir().suggestion(support_message())?;
        }
        _ => {
            unreachable!("this number must stay in [0-4]")
        }
    }

    Ok(())
}

// Helper function to print options to the terminal
fn print_options(
    stdout: &mut io::StdoutLock,
    options: &[String],
    selected: usize,
    position: u16,
) -> io::Result<()> {
    writeln!(
        stdout,
        "{}Please select an option below using arrow keys (or `j` and `k`):\n",
        cursor::Goto(1, position + 2)
    )?;

    // Print options to the terminal
    for (i, option) in options.iter().enumerate() {
        if i == selected {
            let output = format!(" > {} ", option);
            writeln!(stdout, "{} {}", cursor::Goto(1, i as u16 + position + 4), output.green())?;
        } else {
            let output = format!("  {} ", option);
            writeln!(stdout, "{} {}", cursor::Goto(1, i as u16 + position + 4), output)?;
        }
    }
    write!(stdout, "\n\r")?;
    stdout.flush()
}

impl std::fmt::Display for Commands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Commands::Farm { verbose: _, executor: _, debug: _ } => write!(f, "farm"),
            Commands::Wipe { farmer: _, node: _ } => write!(f, "wipe"),
            Commands::Info => write!(f, "info"),
            Commands::Init => write!(f, "init"),
            Commands::OpenLogs => write!(f, "open logs directory"),
        }
    }
}
