use color_eyre::{Report, Result};
use single_instance::SingleInstance;

use crate::summary::{get_farmed_block_count, get_initial_plotting_progress};

pub(crate) async fn info() -> Result<()> {
    let instance = SingleInstance::new("subspaceFarmer").map_err(Report::msg)?;
    if !instance.is_single() {
        println!("A farmer instance is active!");
    } else {
        println!("There is no active farmer instance...");
    }

    println!(
        "Total farmed blocks: {}",
        get_farmed_block_count().await.map_err(|_| Report::msg(
            "Couldn't read the summary file, are you sure you ran the farm command?"
        ))?
    );

    if get_initial_plotting_progress().await.map_err(|_| {
        Report::msg("Couldn't read the summary file, are you sure you ran the farm command?")
    })? {
        println!("Initial plotting is finished!");
    } else {
        println!("Initial plotting is not finished...");
    }

    Ok(())
}
