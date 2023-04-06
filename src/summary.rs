/// Stores the summary of the farming process into a file.
/// This allows to retrieve farming information with `info` command,
/// and also store the amount of potentially farmed blocks during the initial
/// plotting progress, so that progress bar won't be affected with `println!`,
/// and user will still know about them when initial plotting is finished.
use std::{path::PathBuf, sync::Arc};

use bytesize::ByteSize;
use color_eyre::eyre::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::fs::{create_dir_all, read_to_string, remove_file, File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tracing::instrument;

// TODO: delete this when https://github.com/toml-rs/toml/issues/534 is solved
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub(crate) struct Rewards(pub(crate) u128);

impl TryFrom<String> for Rewards {
    type Error = <u128 as std::str::FromStr>::Err;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse().map(Self)
    }
}

impl From<Rewards> for String {
    fn from(Rewards(r): Rewards) -> Self {
        r.to_string()
    }
}

impl std::fmt::Display for Rewards {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Struct for holding the information of what to be displayed with the `info`
/// command
#[derive(Deserialize, Serialize, Debug)]
struct FarmerSummary {
    initial_plotting_finished: bool,
    farmed_block_count: u64,
    vote_count: u64,
    total_rewards: Rewards,
    #[serde(with = "bytesize_serde")]
    user_space_pledged: ByteSize,
}

#[derive(Default, Debug)]
pub(crate) struct SummaryUpdateFields {
    pub(crate) is_plotting_finished: bool,
    pub(crate) is_new_block_farmed: bool,
    pub(crate) is_new_vote: bool,
    pub(crate) maybe_new_reward: Option<Rewards>,
}

/// utilizing persistent storage for the information to be displayed for the
/// `info` command
#[derive(Debug, Clone)]
pub(crate) struct Summary {
    file: Arc<Mutex<PathBuf>>,
}

impl Summary {
    /// creates new summary file
    #[instrument]
    pub(crate) async fn new(user_space_pledged: Option<ByteSize>) -> Result<Summary> {
        let summary_path = summary_path();
        let summary_dir = dirs::data_local_dir()
            .expect("couldn't get the default local data directory!")
            .join("subspace-cli");

        // providing `Some` value for `user_space_pledged` means, we are creating a new
        // file
        if let Some(user_space_pledged) = user_space_pledged {
            // File::create will truncate the existing file, so first
            // check if the file exists, if not, `open` will return an error
            // in this case, create the file and necessary directories
            if File::open(&summary_path).await.is_err() {
                let _ = create_dir_all(&summary_dir).await;
                let _ = File::create(&summary_path).await;
                let initialization = FarmerSummary {
                    initial_plotting_finished: false,
                    farmed_block_count: 0,
                    vote_count: 0,
                    total_rewards: Rewards(0),
                    user_space_pledged,
                };
                let summary_text = toml::to_string(&initialization)
                    .context("Failed to serialize FarmerSummary")?;
                OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(&summary_path)
                    .await?
                    .write_all(summary_text.as_bytes())
                    .await?;
            }
        }

        Ok(Summary { file: Arc::new(Mutex::new(summary_path)) })
    }

    /// updates the summary file
    ///
    /// this function will be called by the farmer when
    /// the status of the `plotting_finished`
    /// or value of `farmed_block_count` changes
    #[instrument]
    pub(crate) async fn update(
        &self,
        SummaryUpdateFields {
            is_plotting_finished,
            is_new_block_farmed,
            is_new_vote,
            maybe_new_reward,
        }: SummaryUpdateFields,
    ) -> Result<()> {
        let mut summary = self.parse_summary().await?;

        if is_plotting_finished {
            summary.initial_plotting_finished = true;
        }
        if is_new_block_farmed {
            summary.farmed_block_count += 1;
        }
        if is_new_vote {
            summary.vote_count += 1;
        }
        if let Some(new_reward) = maybe_new_reward {
            summary.total_rewards = new_reward;
        }

        let new_summary = toml::to_string(&summary).context("Failed to serialize FarmerSummary")?;

        let guard = self.file.lock().await;
        OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&*guard)
            .await?
            .write_all(new_summary.as_bytes())
            .await?;

        Ok(())
    }

    /// retrives how much space has user pledged to the network from the summary
    /// file
    #[instrument]
    pub(crate) async fn get_user_space_pledged(&self) -> Result<ByteSize> {
        let summary = self.parse_summary().await?;
        Ok(summary.user_space_pledged)
    }

    /// retrieves how many blocks have been farmed, from the summary file
    #[instrument]
    pub(crate) async fn get_farmed_block_count(&self) -> Result<u64> {
        let summary = self.parse_summary().await?;
        Ok(summary.farmed_block_count)
    }

    /// retrieves the total amount of rewards in SSC
    #[instrument]
    pub(crate) async fn get_total_rewards(&self) -> Result<Rewards> {
        let summary = self.parse_summary().await?;
        Ok(summary.total_rewards)
    }

    /// retrives how many votes did farmer issue
    #[instrument]
    pub(crate) async fn get_vote_count(&self) -> Result<u64> {
        let summary = self.parse_summary().await?;
        Ok(summary.vote_count)
    }

    /// retrieves the status of the initial plotting, from the summary file
    #[instrument]
    pub(crate) async fn get_initial_plotting_progress(&self) -> Result<bool> {
        let summary = self.parse_summary().await?;
        Ok(summary.initial_plotting_finished)
    }

    /// parses the summary file and returns [`FarmerSummary`]
    #[instrument]
    async fn parse_summary(&self) -> Result<FarmerSummary> {
        let guard = self.file.lock().await;
        let summary: FarmerSummary = toml::from_str(&read_to_string(&*guard).await?)?;
        Ok(summary)
    }
}

/// deletes the summary file
#[instrument]
pub(crate) async fn delete_summary() {
    let _ = remove_file(summary_path()).await;
}

/// returns the path for the summary file
#[instrument]
fn summary_path() -> PathBuf {
    let summary_path =
        dirs::data_local_dir().expect("couldn't get the default local data directory!");
    summary_path.join("subspace-cli").join("summary.toml")
}
