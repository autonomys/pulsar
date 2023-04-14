use color_eyre::eyre::{Context, Result};
use single_instance::SingleInstance;

use crate::commands::farm::SINGLE_INSTANCE;
use crate::summary::{Summary, SummaryFilePointer};

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

    let summary = SummaryFilePointer::new(None).await?;
    let Summary {
        user_space_pledged,
        farmed_block_count,
        vote_count,
        total_rewards,
        initial_plotting_finished,
    } = summary.parse_summary().await.context("couldn't parse summary file")?;

    println!("You have pledged to the network: {user_space_pledged}");

    println!("Farmed {farmed_block_count} block(s)");

    println!("Voted on {vote_count} block(s)");

    println!("{total_rewards} SSC(s) earned!",);

    if initial_plotting_finished {
        println!("Initial plotting is finished!");
    } else {
        println!("Initial plotting is not finished...");
    }

    Ok(())
}
