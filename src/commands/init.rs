use std::{fs::File, io::Write};

use color_eyre::eyre::Result;

use crate::config::{construct_config, create_config};
use crate::utils::{
    get_user_input, is_valid_address, is_valid_chain, is_valid_location, is_valid_node_name,
    is_valid_size, plot_location_getter, print_ascii_art, print_version,
};

/// defaults for the user config file
const DEFAULT_PLOT_SIZE: &str = "100GB";
const DEFAULT_CHAIN: &str = "dev";

/// implementation of the `init` command
///
/// prints a very cool ascii art,
/// creates a config file from the user inputs
pub(crate) fn init() -> Result<()> {
    let (config, config_path) = create_config()?;
    print_ascii_art();
    print_version();
    println!();
    println!("Configuration creation process has started...");
    fill_config_from_user_inputs(config)?;

    println!(
        "Configuration has been generated at {}",
        config_path.display()
    );

    println!("Ready for lift off! Run the follow command to begin:");
    println!("`./subspace-cli farm`");

    Ok(())
}

/// gets the necessary information from user, and writes them to the given configuration file
fn fill_config_from_user_inputs(mut config: File) -> Result<()> {
    let default_plot_loc = plot_location_getter();

    // get user inputs
    let reward_address = get_user_input(
        "Enter your farmer/reward address: ",
        None,
        is_valid_address,
        "Reward address is using an invalid format. Please enter a valid address.",
    )?;

    let default_node_name = whoami::username();
    let node_name = get_user_input(
        &format!("Enter your node name to be identified on the network(defaults to `{}`, press enter to use the default): ", default_node_name),
        Some(&default_node_name),
        is_valid_node_name,
        "Node name cannot include non-ascii characters! Please enter a valid node name.")?;

    let plot_location = get_user_input(
        &format!(
            "Specify a plot location (press enter to use the default: `{}`): ",
            default_plot_loc.display()
        ),
        default_plot_loc.to_str(),
        is_valid_location,
        "supplied directory does not exist! Please enter a valid path.",
    )?;

    let plot_size = get_user_input(
        &format!(
            "Specify a plot size (defaults to `{}`, press enter to use the default): ",
            DEFAULT_PLOT_SIZE
        ),
        Some(DEFAULT_PLOT_SIZE),
        is_valid_size,
        "could not parse the value! Please enter a valid size.",
    )?;

    let chain = get_user_input(
        &format!(
            "Specify the chain to farm(defaults to `{}`, press enter to use the default): ",
            DEFAULT_CHAIN
        ),
        Some(DEFAULT_CHAIN),
        is_valid_chain,
        "given chain is not recognized! Please enter a valid chain.",
    )?;

    let config_text = construct_config(
        &reward_address,
        &plot_location,
        &plot_size,
        &chain,
        &node_name,
    )?;

    // write to config
    config.write_all(config_text.as_bytes())?;
    Ok(())
}
