use std::{io::Write, str::FromStr};

use bytesize::ByteSize;
use color_eyre::eyre::{Context, Result};
use subspace_sdk::{
    farmer::{CacheDescription, DsnBuilder as FarmerDsnBuilder},
    node::{Builder as NodeBuilder, DsnBuilder as NodeDsnBuilder, NetworkBuilder, RpcBuilder},
};

use crate::config::{create_config, ChainConfig, Config, ConfigBuilder, FarmerConfigBuilder};
use crate::utils::{
    cache_directory_getter, get_user_input, node_name_parser, plot_directory_getter,
    plot_directory_parser, print_ascii_art, print_version, reward_address_parser, size_parser,
};

/// defaults for the user config file
const DEFAULT_PLOT_SIZE: bytesize::ByteSize = bytesize::ByteSize::gb(100);

/// implementation of the `init` command
///
/// prints a very cool ascii art,
/// creates a config file from the user inputs
pub(crate) fn init() -> Result<()> {
    let (mut config_file, config_path) = create_config()?;
    print_ascii_art();
    print_version();
    println!();
    println!("Configuration creation process has started...");
    let config = get_config_from_user_inputs()?;
    config_file
        .write_all(
            toml::to_string_pretty(&config)
                .wrap_err("Failed to write config")?
                .as_ref(),
        )
        .wrap_err("Failed to write config")?;

    println!(
        "Configuration has been generated at {}",
        config_path.display()
    );

    println!("Ready for lift off! Run the follow command to begin:");
    println!("`./subspace-cli farm`");

    Ok(())
}

/// gets the necessary information from user, and writes them to the given configuration file
fn get_config_from_user_inputs() -> Result<Config> {
    // GET USER INPUTS...
    // get reward address
    let reward_address = get_user_input(
        "Enter your farmer/reward address: ",
        None,
        reward_address_parser,
    )?;

    // get node name
    let default_node_name = whoami::username();
    let node_name = get_user_input(
        &format!("Enter your node name to be identified on the network(defaults to `{default_node_name}`, press enter to use the default): "),
        Some(default_node_name),
        node_name_parser,
    )?;

    // get plot directory
    let default_plot_loc = plot_directory_getter();
    let plot_directory = get_user_input(
        &format!(
            "Specify a plot location (press enter to use the default: `{default_plot_loc:?}`): ",
        ),
        Some(default_plot_loc),
        plot_directory_parser,
    )?;

    // get plot size
    let plot_size = get_user_input(
        &format!(
            "Specify a plot size (defaults to `{DEFAULT_PLOT_SIZE}`, press enter to use the default): "),
        Some(DEFAULT_PLOT_SIZE),
        size_parser,
    )?;

    // get chain
    let default_chain = ChainConfig::Gemini3a;
    let chain = get_user_input(
        &format!(
            "Specify the chain to farm(defaults to `{default_chain:}`, press enter to use the default): "),
        Some(crate::config::ChainConfig::Gemini3a),
        ChainConfig::from_str,
    )?;

    Ok(ConfigBuilder::default()
        .farmer(
            FarmerConfigBuilder::new()
                .address(reward_address)
                .plot_directory(plot_directory)
                .plot_size(plot_size)
                .cache(CacheDescription::new(
                    cache_directory_getter(),
                    ByteSize::gib(1),
                )?)
                .dsn(
                    FarmerDsnBuilder::new().listen_on(vec!["/ip4/0.0.0.0/tcp/30433"
                        .parse()
                        .expect("hardcoded value is true")]),
                ),
        )
        .node(
            NodeBuilder::new()
                .network(NetworkBuilder::new().name(node_name))
                .dsn(
                    NodeDsnBuilder::new().listen_addresses(vec!["/ip4/0.0.0.0/tcp/30433"
                        .parse()
                        .expect("hardcoded value is true")]),
                )
                .rpc(RpcBuilder::new())
                .configuration(),
        )
        .chain(chain)
        .build())
}
