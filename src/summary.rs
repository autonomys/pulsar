use std::fs::{remove_file, File};
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;

use color_eyre::eyre::{Context, Result};
use derive_more::{AddAssign, Display, From, FromStr};
use serde::{Deserialize, Serialize};
use subspace_sdk::node::BlockNumber;
use subspace_sdk::ByteSize;
use tokio::fs::{create_dir_all, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tracing::instrument;

#[derive(Debug, Clone, Copy, Default, Display, AddAssign, FromStr, From)]
pub(crate) struct Rewards(pub(crate) u128);

#[derive(Default, Debug)]
pub(crate) struct SummaryUpdateFields {
    pub(crate) is_plotting_finished: bool,
    pub(crate) new_authored_count: u64,
    pub(crate) new_vote_count: u64,
    pub(crate) new_reward: Rewards,
    pub(crate) new_parsed_blocks: BlockNumber,
}

#[derive(Deserialize, Serialize, Default, Debug, Clone, Copy)]
pub(crate) struct Summary {
    pub(crate) initial_plotting_finished: bool,
    pub(crate) authored_count: u64,
    pub(crate) vote_count: u64,
    pub(crate) total_rewards: Rewards,
    pub(crate) user_space_pledged: ByteSize,
    pub(crate) last_processed_block_num: BlockNumber,
}

#[derive(Debug, Clone)]
pub(crate) struct SummaryFile {
    inner: Arc<Mutex<File>>,
}

impl SummaryFile {
    #[instrument]
    pub(crate) async fn new(user_space_pledged: Option<ByteSize>) -> Result<SummaryFile> {
        let summary_path = summary_path();
        let summary_dir = summary_dir();

        let mut summary_file;
        if let Some(user_space_pledged) = user_space_pledged {
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
                let summary_text = toml::to_string(&initialization)
                    .context("Failed to serialize Summary")?;
                summary_file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .truncate(true)
                    .open(&summary_path)
                    .await
                    .context("couldn't open new summary file")?;
                summary_file
                    .write_all(summary_text.as_bytes())
                    .await
                    .context("write to summary failed")?;
                summary_file.flush().await.context("flush at creation failed")?;
                summary_file
                    .seek(SeekFrom::Start(0))
                    .await
                    .context("couldn't seek to the beginning of the summary file")?;

                return Ok(SummaryFile { inner: Arc::new(Mutex::new(summary_file)) });
            }
        }
        summary_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&summary_path)
            .await
            .context("couldn't open existing summary file")?;
        Ok(SummaryFile { inner: Arc::new(Mutex::new(summary_file)) })
    }

    #[instrument]
    pub(crate) async fn parse(&self) -> Result<Summary> {
        let mut guard = self.inner.lock().await;
        let mut contents = String::new();

        guard
            .read_to_string(&mut contents)
            .await
            .context("couldn't read the contents of the summary file")?;
        let summary: Summary =
            toml::from_str(&contents).context("couldn't serialize the summary content")?;

        guard
            .seek(SeekFrom::Start(0))
            .await
            .context("couldn't seek to the beginning of the summary file")?;

        Ok(summary)
    }

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
        let mut summary = self.parse().await.context("couldn't parse summary in update method")?;

        if is_plotting_finished {
            summary.initial_plotting_finished = true;
        }

        summary.authored_count += new_authored_count;
        summary.vote_count += new_vote_count;
        summary.total_rewards += new_reward;
        summary.last_processed_block_num += new_parsed_blocks;

        let serialized_summary =
            toml::to_string(&summary).context("Failed to serialize Summary")?;
        let mut guard = self.inner.lock().await;
        guard.set_len(0).await.context("couldn't truncate the summary file")?;
        guard
            .write_all(serialized_summary.as_bytes())
            .await
            .context("couldn't write to summary file")?;
        guard
            .seek(SeekFrom::Start(0))
            .await
            .context("couldn't seek to the beginning of the summary file")?;
        guard.flush().await.context("flushing failed for summary file")?;

        Ok(summary)
    }
}

#[instrument]
pub(crate) fn delete_summary() -> Result<()> {
    remove_file(summary_path()).context("couldn't delete summary file")
}

#[instrument]
pub(crate) fn summary_path() -> PathBuf {
    summary_dir().join("summary.toml")
}

#[instrument]
fn summary_dir() -> PathBuf {
    dirs::cache_dir()
        .expect("couldn't get the directory!")
        .join("subspace-cli")
}
