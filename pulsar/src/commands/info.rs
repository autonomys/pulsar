use color_eyre::eyre::{Context, Result};
use single_instance::SingleInstance;

use crate::commands::farm::SINGLE_INSTANCE;
use crate::summary::{Summary, SummaryFile};

/// implementation of the `init` command.
///
/// informs the user about the current farming instance
pub(crate) async fn info() -> Result<()> {
    let instance =
        SingleInstance::new(SINGLE_INSTANCE).context("failed to initialize single instance")?;
    if !instance.is_single() {
        println!("A farmer instance is active!");
    } else {
        println!("There is no active farmer instance...");
    }

    let summary_file = SummaryFile::new(None).await?;
    let Summary {
        user_space_pledged,
        authored_count,
        vote_count,
        total_rewards,
        initial_plotting_finished,
        last_processed_block_num: last_block_parsed,
    } = summary_file
        .parse()
        .await
        .context("couldn't parse summary file, are you sure you have ran `farm` command?")?;

    println!("You have pledged to the network: {user_space_pledged}");

    println!("Farmed {authored_count} block(s)");

    println!("Voted on {vote_count} block(s)");

    println!("{total_rewards} SSC(s) earned!",);

    println!("This data is derived from the first {last_block_parsed} blocks in the chain!",);

    if initial_plotting_finished {
        println!("Initial plotting is finished!");
    } else {
        println!("Initial plotting is not finished...");
    }

    Ok(())
}
