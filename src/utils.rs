use std::env;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use color_eyre::eyre::{eyre, Context, Result};
use futures::prelude::*;
use owo_colors::OwoColorize;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use subspace_sdk::{ByteSize, PublicKey};
use tracing::level_filters::LevelFilter;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_error::ErrorLayer;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter, Layer};

use crate::config::MIN_PLOT_SIZE;
use crate::summary::Rewards;

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
    let exec_name =
        std::env::args().next().map(PathBuf::from).expect("First argument always exists");
    println!("`{exec_name:?} farm`");
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
pub(crate) fn directory_parser(location: &str) -> Result<PathBuf> {
    let path = Path::new(location).to_owned();
    if path.is_dir() {
        Ok(path)
    } else {
        // prompt the user for creation of the given path
        let prompt = "The given path does not exist. Do you want to create it? (y/n): ";
        let permission = get_user_input(prompt, None, yes_or_no_parser).context("prompt failed")?;
        if permission {
            create_dir_all(&path)?;
            return Ok(path);
        }
        Err(eyre!("supplied directory does not exist! Please enter a valid path."))
    }
}

/// utilize `ByteSize` crate for the validation
pub(crate) fn size_parser(size: &str) -> Result<ByteSize> {
    let Ok(size) = size.parse::<ByteSize>() else {
         return Err(eyre!("could not parse the value!"));
    };
    if size < MIN_PLOT_SIZE {
        Err(eyre!(format!("plot size cannot be smaller than {}", MIN_PLOT_SIZE)))
    } else {
        Ok(size)
    }
}

pub(crate) fn yes_or_no_parser(answer: &str) -> Result<bool> {
    match answer.to_lowercase().as_str() {
        "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        _ => Err(eyre!("could not interpret your answer. Please provide `y` or `n`.")),
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
    dirs::data_dir().expect("data folder must be present in every major OS").join("pulsar")
}

/// returns OS specific log directory
pub(crate) fn custom_log_dir() -> PathBuf {
    let id = "pulsar";

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

/// opens log directory
pub(crate) fn open_log_dir() -> Result<()> {
    let path = custom_log_dir();
    open::that(path).context("couldn't open the directory")
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
pub(crate) fn install_tracing(is_verbose: bool, no_rotation: bool) {
    let log_dir = custom_log_dir();
    let _ = create_dir_all(&log_dir);

    let file_appender = if no_rotation {
        RollingFileAppender::builder().rotation(Rotation::NEVER)
    } else {
        RollingFileAppender::builder().max_log_files(KEEP_LAST_N_FILE).rotation(Rotation::HOURLY)
    }
    .filename_prefix("pulsar.log")
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
    #[cfg(tokio_unstable)]
    let tracing_layer = tracing_subscriber::registry().with(console_subscriber::spawn());

    #[cfg(not(tokio_unstable))]
    let tracing_layer = tracing_subscriber::registry();

    let tracing_layer = tracing_layer
        .with(
            BunyanFormattingLayer::new("pulsar".to_owned(), file_appender)
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

pub fn apply_extra_options<T: serde::Serialize + serde::de::DeserializeOwned>(
    config: &T,
    extra: toml::Table,
) -> Result<T> {
    fn apply_extra_options_inner(config: &mut toml::Table, extra: toml::Table) {
        for (k, v) in extra {
            use toml::Value::Table;

            let e = match config.get_mut(&k) {
                Some(e) => e,
                None => {
                    config.insert(k, v);
                    continue;
                }
            };

            match (e, v) {
                (Table(table), Table(v)) => apply_extra_options_inner(table, v),
                (entry, v) => *entry = v,
            }
        }
    }

    let mut table: toml::Table =
        toml::from_str(&toml::to_string(config).expect("Config is always toml serializable"))
            .expect("Config is always toml deserializable");

    apply_extra_options_inner(&mut table, extra);

    Ok(toml::from_str(&toml::to_string(&table).context("Failed to deserialize extra options")?)
        .expect("At this stage we know that config is always toml deserializable"))
}

#[cfg(tokio_unstable)]
pub(crate) fn spawn_task<F>(name: impl AsRef<str>, future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: futures::Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::task::Builder::new()
        .name(name.as_ref())
        .spawn(future)
        .expect("Spawning task never fails")
}

#[cfg(not(tokio_unstable))]
pub(crate) fn spawn_task<F>(name: impl AsRef<str>, future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: futures::Future + Send + 'static,
    F::Output: Send + 'static,
{
    let _ = name;
    tokio::task::spawn(future)
}

fn into_eyre_err(err: anyhow::Error) -> color_eyre::eyre::Error {
    eyre!(Box::<dyn std::error::Error + Send + Sync>::from(err))
}

pub trait IntoEyre: Sized {
    type Ok;
    fn into_eyre(self) -> Result<Self::Ok>;
}

impl<T> IntoEyre for anyhow::Result<T> {
    type Ok = T;

    fn into_eyre(self) -> Result<T> {
        self.map_err(into_eyre_err)
    }
}

pub trait IntoEyreFuture: TryFuture<Error = anyhow::Error> + Sized {
    fn into_eyre(
        self,
    ) -> futures::future::MapErr<Self, fn(anyhow::Error) -> color_eyre::eyre::Error> {
        use futures::TryFutureExt;

        self.map_err(into_eyre_err)
    }
}

impl<T> IntoEyreFuture for T where T: TryFuture<Error = anyhow::Error> + Sized {}

pub trait IntoEyreStream: TryStream<Error = anyhow::Error> + Sized {
    fn into_eyre(
        self,
    ) -> futures::stream::MapErr<Self, fn(anyhow::Error) -> color_eyre::eyre::Error> {
        use futures::TryStreamExt;

        self.map_err(into_eyre_err)
    }
}

impl<T> IntoEyreStream for T where T: TryStream<Error = anyhow::Error> + Sized {}

impl Serialize for Rewards {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // You can choose how you want to serialize the data.
        // Here, we're just serializing the u128 as a string.
        let s = self.0.to_string();
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for Rewards {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Choose how you want to deserialize the data.
        // Here, we're deserializing the string back to u128.
        let s = String::deserialize(deserializer)?;
        let value = s.parse::<u128>().map_err(serde::de::Error::custom)?;
        Ok(Rewards(value))
    }
}
