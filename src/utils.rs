use std::env;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use bytesize::ByteSize;
use color_eyre::eyre::{eyre, Context, Result};
use owo_colors::OwoColorize;
use subspace_sdk::PublicKey;
use tracing::level_filters::LevelFilter;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_error::ErrorLayer;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter, Layer};

use crate::config::MIN_PLOT_SIZE;

/// for how long a log file should be valid
const KEEP_LAST_N_FILE: usize = 7;

/// <3
pub(crate) fn print_ascii_art() {
    println!("
 ____             __                                              __  __          __                               __
/\\  _`\\          /\\ \\                                            /\\ \\/\\ \\        /\\ \\__                           /\\ \\
\\ \\,\\L\\_\\  __  __\\ \\ \\____    ____  _____      __      ___     __\\ \\ `\\\\ \\     __\\ \\ ,_\\  __  __  __    ___   _ __\\ \\ \\/'\\
 \\/_\\__ \\ /\\ \\/\\ \\\\ \\ '__`\\  /',__\\/\\ '__`\\  /'__`\\   /'___\\ /'__`\\ \\ , ` \\  /'__`\\ \\ \\/ /\\ \\/\\ \\/\\ \\  / __`\\/\\`'__\\ \\ , <
   /\\ \\L\\ \\ \\ \\_\\ \\\\ \\ \\L\\ \\/\\__, `\\ \\ \\L\\ \\/\\ \\L\\.\\_/\\ \\__//\\  __/\\ \\ \\`\\ \\/\\  __/\\ \\ \\_\\ \\ \\_/ \\_/ \\/\\ \\L\\ \\ \\ \\/ \\ \\ \\\\`\\
   \\ `\\____\\ \\____/ \\ \\_,__/\\/\\____/\\ \\ ,__/\\ \\__/.\\_\\ \\____\\ \\____\\\\ \\_\\ \\_\\ \\____\\\\ \\__\\\\ \\___x___/'\\ \\____/\\ \\_\\  \\ \\_\\ \\_\\
    \\/_____/\\/___/   \\/___/  \\/___/  \\ \\ \\/  \\/__/\\/_/\\/____/\\/____/ \\/_/\\/_/\\/____/ \\/__/ \\/__//__/   \\/___/  \\/_/   \\/_/\\/_/
                                      \\ \\_\\
                                       \\/_/
");
}

/// prints the version of the crate
pub(crate) fn print_version() {
    let version: &str = env!("CARGO_PKG_VERSION");
    println!("version: {version}");
}

pub(crate) fn print_run_executable_command() {
    let executable_name = format!(
        "subspace-cli-{}-{}-{}-alpha",
        env::consts::OS,
        env::consts::ARCH,
        env!("CARGO_PKG_VERSION")
    );

    #[cfg(target_os = "windows")]
    let executable_name = format!("{executable_name}.exe");

    let command = format!("`./{executable_name} farm`");

    println!("{command}");
}

/// gets the input from the user for a given `prompt`
///
/// `default_value`: will be used if user does not provide any input
///
/// `condition`: will be checked against for the user input,
/// the user will be repeatedly prompted to provide a valid input
///
/// `error_msg`: will be displayed if user enters an input which does not
/// satisfy the `condition`
pub(crate) fn get_user_input<F, O, E>(
    prompt: &str,
    default_value: Option<O>,
    condition: F,
) -> Result<O>
where
    E: std::fmt::Display,
    F: Fn(&str) -> Result<O, E>,
{
    loop {
        print!("{prompt}");
        std::io::Write::flush(&mut std::io::stdout())?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let user_input = input.trim().to_string();

        // Allow this unwrap cause
        #[allow(clippy::unnecessary_unwrap, clippy::unwrap_used)]
        if default_value.is_some() && user_input.is_empty() {
            return Ok(default_value.unwrap());
        }

        match condition(&user_input) {
            Ok(o) => return Ok(o),
            Err(err) => println!("{err}"),
        }
    }
}

/// node name should be ascii, and should begin/end with whitespace
pub(crate) fn node_name_parser(node_name: &str) -> Result<String> {
    let node_name = node_name.trim();
    match node_name {
        "" => Err(eyre!("Node name cannot be empty!")),
        "root" => Err(eyre!("please select a name different than `root`")),
        _ => Ok(node_name.to_string()),
    }
}

/// check for a valid SS58 address
pub(crate) fn reward_address_parser(address: &str) -> Result<PublicKey> {
    PublicKey::from_str(address).context("Failed to parse reward address")
}

/// the provided path should be an existing directory
pub(crate) fn plot_directory_parser(location: &str) -> Result<PathBuf> {
    let path = Path::new(location).to_owned();
    if path.is_dir() {
        Ok(path)
    } else {
        Err(eyre!("supplied directory does not exist! Please enter a valid path."))
    }
}

/// utilize `ByteSize` crate for the validation
pub(crate) fn size_parser(size: &str) -> Result<ByteSize> {
    let Ok(size) = size.parse::<ByteSize>() else {
         return Err(eyre!("could not parse the value!"));
    };
    if size < MIN_PLOT_SIZE {
        Err(eyre!("size could not be smaller than 1GB"))
    } else {
        Ok(size)
    }
}

/// generates a plot path from the given path
pub(crate) fn plot_directory_getter() -> PathBuf {
    data_dir_getter().join("plots")
}

/// generates a cache path from the given path
pub(crate) fn cache_directory_getter() -> PathBuf {
    data_dir_getter().join("cache")
}

/// generates a node path from the given path
pub(crate) fn node_directory_getter() -> PathBuf {
    data_dir_getter().join("node")
}

pub(crate) fn provider_storage_dir_getter() -> PathBuf {
    node_directory_getter().join("provider-storage")
}

fn data_dir_getter() -> PathBuf {
    dirs::data_dir().expect("data folder must be present in every major OS").join("subspace-cli")
}

/// returns OS specific log directory
fn custom_log_dir() -> PathBuf {
    let id = "subspace-cli";

    #[cfg(target_os = "macos")]
    let path = dirs::home_dir().map(|dir| dir.join("Library/Logs").join(id));
    // evaluates to: `~/Library/Logs/{id}/

    #[cfg(target_os = "linux")]
    let path = dirs::data_local_dir().map(|dir| dir.join(id).join("logs"));
    // evaluates to: `~/.local/share/${id}/logs/

    #[cfg(target_os = "windows")]
    let path = dirs::data_local_dir().map(|dir| dir.join(id).join("logs"));
    // evaluates to: `C:/Users/Username/AppData/Local/${id}/logs/

    path.expect("Could not resolve custom log directory path!")
}

/// in case of any error, display this message
pub(crate) fn support_message() -> String {
    format!(
        "If you think this is a bug, please submit it to our forums: {}",
        "https://forum.subspace.network".underline()
    )
}

pub(crate) fn raise_fd_limit() {
    match std::panic::catch_unwind(fdlimit::raise_fd_limit) {
        Ok(Some(limit)) => {
            tracing::info!("Increase file limit from soft to hard (limit is {limit})")
        }
        Ok(None) => tracing::debug!("Failed to increase file limit"),
        Err(err) => {
            let err = if let Some(err) = err.downcast_ref::<&str>() {
                *err
            } else if let Some(err) = err.downcast_ref::<String>() {
                err
            } else {
                unreachable!(
                    "Should be unreachable as `fdlimit` uses panic macro, which should return \
                     either `&str` or `String`."
                )
            };
            tracing::warn!("Failed to increase file limit: {err}")
        }
    }
}

/// install a logger for the application
pub(crate) fn install_tracing(is_verbose: bool) {
    let log_dir = custom_log_dir();
    let _ = create_dir_all(&log_dir);

    let file_appender = RollingFileAppender::builder()
        .max_log_files(KEEP_LAST_N_FILE)
        .rotation(Rotation::HOURLY)
        .filename_prefix("subspace-cli.log")
        .build(log_dir)
        .expect("building should always succeed");

    // filter for logging
    let filter = || {
        EnvFilter::builder()
            .with_default_directive(LevelFilter::INFO.into())
            .from_env_lossy()
            .add_directive("regalloc2=off".parse().expect("hardcoded value is true"))
    };

    // start logger, after we acquire the bundle identifier
    let tracing_layer = tracing_subscriber::registry()
        .with(
            BunyanFormattingLayer::new("subspace-cli".to_owned(), file_appender)
                .and_then(JsonStorageLayer)
                .with_filter(filter()),
        )
        .with(ErrorLayer::default());

    // if verbose, then also print to stdout
    if is_verbose {
        tracing_layer
            .with(
                fmt::layer()
                    .with_ansi(!cfg!(windows))
                    .with_span_events(FmtSpan::CLOSE)
                    .with_filter(filter()),
            )
            .init();
    } else {
        tracing_layer.init();
    }
}

pub fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ChainConfig;

    #[test]
    fn node_name_checker() {
        assert!(node_name_parser("     ").is_err());
        assert!(node_name_parser("root ").is_err());
        assert!(node_name_parser("ゴゴゴゴ yare yare daze").is_ok());
    }

    #[test]
    fn reward_address_checker() {
        // below address is randomly generated via metamask and then deleted
        assert!(reward_address_parser("5FWr7j9DW4uy7K1JLmFN2R3eoae35PFDUfW7G42ARpBEUaN7").is_ok());
        assert!(reward_address_parser("sdjhfskjfhdksjhfsfhskjskdjhfdsfjhk").is_err());
    }

    #[test]
    fn size_checker() {
        assert!(size_parser("800MB").is_ok());
        assert!(size_parser("103gjie").is_err());
        assert!(size_parser("12GB").is_ok());
    }

    #[test]
    fn chain_checker() {
        assert!(ChainConfig::from_str("gemini-3c").is_ok());
        assert!(ChainConfig::from_str("devv").is_err());
    }

    #[test]
    fn plot_directory_tester() {
        let plot_path = plot_directory_getter();

        #[cfg(target_os = "macos")]
        assert!(plot_path.ends_with("Library/Application Support/subspace-cli/plots"));

        #[cfg(target_os = "linux")]
        assert!(plot_path.ends_with(".local/share/subspace-cli/plots"));

        #[cfg(target_os = "windows")]
        assert!(plot_path.ends_with("AppData/Roaming/subspace-cli/plots"));
    }

    #[test]
    fn cache_directory_tester() {
        let cache_path = cache_directory_getter();

        #[cfg(target_os = "macos")]
        assert!(cache_path.ends_with("Library/Application Support/subspace-cli/cache"));

        #[cfg(target_os = "linux")]
        assert!(cache_path.ends_with(".local/share/subspace-cli/cache"));

        #[cfg(target_os = "windows")]
        assert!(cache_path.ends_with("AppData/Roaming/subspace-cli/cache"));
    }

    #[test]
    fn node_directory_tester() {
        let node_path = node_directory_getter();

        #[cfg(target_os = "macos")]
        assert!(node_path.ends_with("Library/Application Support/subspace-cli/node"));

        #[cfg(target_os = "linux")]
        assert!(node_path.ends_with(".local/share/subspace-cli/node"));

        #[cfg(target_os = "windows")]
        assert!(node_path.ends_with("AppData/Roaming/subspace-cli/node"));
    }

    #[test]
    fn custom_log_dir_test() {
        let log_path = custom_log_dir();

        #[cfg(target_os = "macos")]
        assert!(log_path.ends_with("Library/Logs/subspace-cli"));

        #[cfg(target_os = "linux")]
        assert!(log_path.ends_with(".local/share/subspace-cli/logs"));

        #[cfg(target_os = "windows")]
        assert!(log_path.ends_with("AppData/Local/subspace-cli/logs"));
    }
}
