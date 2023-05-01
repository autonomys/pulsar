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
use serde::{Deserialize, Serialize};
use subspace_sdk::node::BlockNumber;
use subspace_sdk::ByteSize;
use tokio::fs::{create_dir_all, read_to_string, File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tracing::instrument;

// TODO: delete this when https://github.com/toml-rs/toml/issues/540 is solved
#[derive(Debug, Clone, Copy, Default, Display, AddAssign, FromStr, From)]
pub(crate) struct Rewards(pub(crate) u128);

/// struct for updating the fields of the summary
#[derive(Default, Debug)]
pub(crate) struct SummaryUpdateFields {
    pub(crate) is_plotting_finished: bool,
    pub(crate) new_authored_count: u64,
    pub(crate) new_vote_count: u64,
    pub(crate) new_reward: Rewards,
    pub(crate) new_parsed_blocks: BlockNumber,
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
    inner: Arc<Mutex<PathBuf>>,
}

impl SummaryFile {
    /// creates new summary file pointer
    ///
    /// if user_space_pledged is provided, it creates a new summary file
    /// else, it tries to read the existing summary file
    #[instrument]
    pub(crate) async fn new(user_space_pledged: Option<ByteSize>) -> Result<SummaryFile> {
        let summary_path = summary_path();
        let summary_dir = dirs::cache_dir()
            .expect("couldn't get the default local data directory!")
            .join("subspace-cli");

        // providing `Some` value for `user_space_pledged` means, we are creating a new
        // file, so, first check if the file exists to not erase its content
        if let Some(user_space_pledged) = user_space_pledged {
            // File::create will truncate the existing file, so first
            // check if the file exists, if not, `open` will return an error
            // in this case, create the file and necessary directories
            // if file exists, we do nothing
            if File::open(&summary_path).await.is_err() {
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
                OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(&summary_path)
                    .await?
                    .write_all(summary_text.as_bytes())
                    .await?;
            }
        }

        Ok(SummaryFile { inner: Arc::new(Mutex::new(summary_path)) })
    }

    /// parses the summary file and returns [`Summary`]
    #[instrument]
    pub(crate) async fn parse(&self) -> Result<Summary> {
        let guard = self.inner.lock().await;
        let inner: Summary = toml::from_str(&read_to_string(&*guard).await?)?;

        Ok(inner)
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
        }: SummaryUpdateFields,
    ) -> Result<Summary> {
        let mut summary: Summary = Default::default();

        if is_plotting_finished {
            summary.initial_plotting_finished = true;
        }

        summary.authored_count += new_authored_count;

        summary.vote_count += new_vote_count;

        summary.total_rewards += new_reward;

        summary.last_processed_block_num += new_parsed_blocks;

        let serialized_summary =
            toml::to_string(&summary).context("Failed to serialize Summary")?;
        // this will only create the pointer, and will not override the file
        let guard = self.inner.lock().await;
        let mut buffer = OpenOptions::new().write(true).truncate(true).open(&*guard).await?;
        buffer.write_all(serialized_summary.as_bytes()).await?;
        buffer.flush().await?;

        Ok(summary)
    }
}

/// deletes the summary file
#[instrument]
pub(crate) fn delete_summary() -> Result<()> {
    remove_file(summary_path()).context("couldn't delete summary file")
}

/// returns the path for the summary file
#[instrument]
fn summary_path() -> PathBuf {
    let summary_path =
        dirs::data_local_dir().expect("couldn't get the default local data directory!");
    summary_path.join("subspace-cli").join("summary.toml")
}
