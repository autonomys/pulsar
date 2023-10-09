/// Stores the summary of the farming process into a file.
/// This allows to retrieve farming information with `info` command,
/// and also store the amount of potentially farmed blocks during the initial
/// plotting progress, so that progress bar won't be affected with `println!`,
/// and user will still know about them when initial plotting is finished.
use std::fs::remove_file;
use std::path::PathBuf;
use std::sync::Arc;

use color_eyre::eyre::{Context, Result};
use derive_more::{AddAssign, Display, From, FromStr};
use num_rational::Ratio;
use num_traits::cast::ToPrimitive;
use serde::{Deserialize, Serialize};
use subspace_sdk::node::BlockNumber;
use subspace_sdk::ByteSize;
use tokio::fs::{create_dir_all, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{Mutex, MutexGuard};
use tracing::instrument;

// TODO: delete this when https://github.com/toml-rs/toml/issues/540 is solved
#[derive(Debug, Clone, Copy, Default, Display, AddAssign, FromStr, From)]
pub(crate) struct Rewards(pub(crate) u128);

impl Rewards {
    /// Converts the reward amount to SSC by dividing by 10^18.
    pub(crate) fn as_ssc(&self) -> f64 {
        Ratio::new(self.0, 10u128.pow(18)).to_f64().expect("10u128.pow(18) is never 0; qed")
    }
}

/// struct for updating the fields of the summary
#[derive(Default, Debug)]
pub(crate) struct SummaryUpdateFields {
    pub(crate) is_plotting_finished: bool,
    pub(crate) new_authored_count: u64,
    pub(crate) new_vote_count: u64,
    pub(crate) new_reward: Rewards,
    pub(crate) new_parsed_blocks: BlockNumber,
    pub(crate) maybe_updated_user_space_pledged: Option<ByteSize>,
}

/// Struct for holding the info of what to be displayed with the `info` command,
/// and printing rewards to user in `farm` command
#[derive(Deserialize, Serialize, Default, Debug, Clone, Copy)]
pub(crate) struct Summary {
    pub(crate) initial_plotting_finished: bool,
    pub(crate) authored_count: u64,
    pub(crate) vote_count: u64,
    pub(crate) total_rewards: Rewards,
    pub(crate) user_space_pledged: ByteSize,
    pub(crate) last_processed_block_num: BlockNumber,
}

/// utilizing persistent storage for the information to be displayed for the
/// `info` command
#[derive(Debug, Clone)]
pub(crate) struct SummaryFile {
    inner: Arc<Mutex<File>>,
}

impl SummaryFile {
    /// creates a new summary file Mutex
    ///
    /// if user_space_pledged is provided, it creates a new summary file
    /// else, it tries to open the existing summary file
    #[instrument]
    pub(crate) async fn new(maybe_user_space_pledged: Option<ByteSize>) -> Result<SummaryFile> {
        let summary_path = summary_path();
        let summary_dir = summary_dir();

        let file_handle = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&summary_path)
            .await
            .context("couldn't open existing summary file");
        match file_handle {
            Err(e) => {
                let user_space_pledged = match maybe_user_space_pledged {
                    Some(user_space_pledged) => user_space_pledged,
                    // As per the API contract, if user_space_pledged is None, we only want to open
                    // existing summary file
                    None => return Err(e),
                };
                let _ = create_dir_all(&summary_dir).await;
                let _ = File::create(&summary_path).await;
                let initialization = Summary {
                    initial_plotting_finished: false,
                    authored_count: 0,
                    vote_count: 0,
                    total_rewards: Rewards(0),
                    user_space_pledged,
                    last_processed_block_num: 0,
                };
                let summary_text =
                    toml::to_string(&initialization).context("Failed to serialize Summary")?;
                let mut summary_file_handle = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .truncate(true)
                    .open(&summary_path)
                    .await
                    .context("couldn't open new summary file")?;
                summary_file_handle
                    .write_all(summary_text.as_bytes())
                    .await
                    .context("write to summary failed")?;
                summary_file_handle.flush().await.context("flush at creation failed")?;
                summary_file_handle
                    .seek(std::io::SeekFrom::Start(0))
                    .await
                    .context("couldn't seek to the beginning of the summary file")?;

                Ok(SummaryFile { inner: Arc::new(Mutex::new(summary_file_handle)) })
            }
            Ok(summary_file_handle) => {
                let summary_file = SummaryFile { inner: Arc::new(Mutex::new(summary_file_handle)) };
                if let Some(user_space_pledged) = maybe_user_space_pledged {
                    let (summary, _) = summary_file.read_and_deserialize().await?;
                    summary_file
                        .update(SummaryUpdateFields {
                            is_plotting_finished: summary.initial_plotting_finished,
                            maybe_updated_user_space_pledged: Some(user_space_pledged),
                            // No change in other values
                            new_authored_count: 0,
                            new_vote_count: 0,
                            new_reward: Rewards(0),
                            new_parsed_blocks: 0,
                        })
                        .await?;
                }
                Ok(summary_file)
            }
        }
    }

    /// Parses the summary file and returns [`Summary`]
    #[instrument]
    pub(crate) async fn parse(&self) -> Result<Summary> {
        let (summary, _) = self.read_and_deserialize().await?;
        Ok(summary)
    }

    /// updates the summary file, and returns the content of the new summary
    ///
    /// this function will be called by the farmer when
    /// the status of the `plotting_finished`
    /// or value of `farmed_block_count` changes
    #[instrument]
    pub(crate) async fn update(
        &self,
        SummaryUpdateFields {
            is_plotting_finished,
            new_authored_count,
            new_vote_count,
            new_reward,
            new_parsed_blocks,
            maybe_updated_user_space_pledged,
        }: SummaryUpdateFields,
    ) -> Result<Summary> {
        let (mut summary, mut guard) = self.read_and_deserialize().await?;

        if is_plotting_finished {
            summary.initial_plotting_finished = true;
        }

        summary.authored_count += new_authored_count;

        summary.vote_count += new_vote_count;

        summary.total_rewards += new_reward;

        summary.last_processed_block_num += new_parsed_blocks;

        if let Some(updated_user_space_pledged) = maybe_updated_user_space_pledged {
            summary.user_space_pledged = updated_user_space_pledged;
        }

        let serialized_summary =
            toml::to_string(&summary).context("Failed to serialize Summary")?;

        guard.set_len(0).await.context("couldn't truncate the summary file")?;
        guard
            .write_all(serialized_summary.as_bytes())
            .await
            .context("couldn't write to summary file")?;
        guard.flush().await.context("flushing failed for summary file")?;
        guard
            .seek(std::io::SeekFrom::Start(0))
            .await
            .context("couldn't seek to the beginning of the summary file")?;

        Ok(summary)
    }

    /// Reads the file, serializes it into `Summary` and seeks to the beginning
    /// of the file
    #[instrument]
    async fn read_and_deserialize(&self) -> Result<(Summary, MutexGuard<'_, File>)> {
        let mut guard = self.inner.lock().await;
        let mut contents = String::new();

        guard
            .read_to_string(&mut contents)
            .await
            .context("couldn't read the contents of the summary file")?;
        let summary: Summary =
            toml::from_str(&contents).context("couldn't serialize the summary content")?;

        guard
            .seek(std::io::SeekFrom::Start(0))
            .await
            .context("couldn't seek to the beginning of the summary file")?;

        Ok((summary, guard))
    }
}

/// deletes the summary file
#[instrument]
pub(crate) fn delete_summary() -> Result<()> {
    remove_file(summary_path()).context("couldn't delete summary file")
}

/// returns the path for the summary file
#[instrument]
pub(crate) fn summary_path() -> PathBuf {
    summary_dir().join("summary.toml")
}

#[instrument]
fn summary_dir() -> PathBuf {
    dirs::cache_dir().expect("couldn't get the  directory!").join("pulsar")
}
