/// Stores the summary of the farming process into a file.
/// This allows to retrieve farming information with `info` command,
/// and also store the amount of potentially farmed blocks during the initial plotting progress,
/// so that progress bar won't be affected with `println!`, and user will still know about them
/// when initial plotting is finished.
use std::path::PathBuf;

use bytesize::ByteSize;
use color_eyre::eyre::{Report, Result};
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{create_dir_all, read_to_string, File, OpenOptions},
    io::AsyncWriteExt,
};
use tracing::instrument;

/// Struct for holding the information of what to be displayed with the `info` command
#[derive(Deserialize, Serialize, Debug)]
struct FarmerSummary {
    initial_plotting_finished: bool,
    farmed_block_count: u64,
    #[serde(with = "bytesize_serde")]
    user_space_pledged: ByteSize,
}

/// utilizing persistent storage for the information to be displayed for the `info` command
#[instrument]
pub(crate) async fn create_summary_file(user_space_pledged: ByteSize) -> Result<()> {
    let summary_path = summary_path();
    let summary_dir = dirs::data_local_dir()
        .expect("couldn't get the default local data directory!")
        .join("subspace-cli");

    // File::create will truncate the existing file, so first
    // check if the file exists, if not, `open` will return an error
    // in this case, create the file and necessary directories
    if File::open(&summary_path).await.is_err() {
        let _ = create_dir_all(&summary_dir).await;
        let _ = File::create(&summary_path).await;
        let initialization = FarmerSummary {
            initial_plotting_finished: false,
            farmed_block_count: 0,
            user_space_pledged,
        };
        let summary = toml::to_string(&initialization).map_err(Report::msg)?;
        OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(summary_path)
            .await?
            .write_all(summary.as_bytes())
            .await?;
    }

    Ok(())
}

/// updates the summary file
///
/// this function will be called by the farmer when
/// the status of the [`plotting_finished`]
/// or value of [`farmed_block_count`] changes
#[instrument]
pub(crate) async fn update_summary(
    plotting_finished: Option<bool>,
    farmed_block_count: Option<u64>,
) -> Result<()> {
    let summary_path = summary_path();
    let mut summary = parse_summary(&summary_path).await?;
    if let Some(flag) = plotting_finished {
        summary.initial_plotting_finished = flag;
    }
    if let Some(count) = farmed_block_count {
        summary.farmed_block_count = count;
    }

    let new_summary = toml::to_string(&summary).map_err(Report::msg)?;

    OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(summary_path)
        .await?
        .write_all(new_summary.as_bytes())
        .await?;

    Ok(())
}

/// retrives how much space has user pledged to the network from the summary file
pub(crate) async fn get_user_space_pledged() -> Result<ByteSize> {
    let summary = parse_summary(&summary_path()).await?;
    Ok(summary.user_space_pledged)
}

/// retrieves how many blocks have been farmed, from the summary file
pub(crate) async fn get_farmed_block_count() -> Result<u64> {
    let summary = parse_summary(&summary_path()).await?;
    Ok(summary.farmed_block_count)
}

/// retrieves the status of the initial plotting, from the summary file
pub(crate) async fn get_initial_plotting_progress() -> Result<bool> {
    let summary = parse_summary(&summary_path()).await?;
    Ok(summary.initial_plotting_finished)
}

/// parses the summary file and returns [`FarmerSummary`]
#[instrument]
async fn parse_summary(path: &PathBuf) -> Result<FarmerSummary> {
    let summary: FarmerSummary = toml::from_str(&read_to_string(path).await?)?;
    Ok(summary)
}

/// returns the path for the summary file
#[instrument]
fn summary_path() -> PathBuf {
    let summary_path =
        dirs::data_local_dir().expect("couldn't get the default local data directory!");
    summary_path.join("subspace-cli").join("summary.toml")
}
