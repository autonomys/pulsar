use std::io::Write;
use std::path::PathBuf;

use color_eyre::eyre::{eyre, Context, Result};

use crate::config::{create_config, ChainConfig, Config, FarmerConfig, NodeConfig};
use crate::utils::{
    get_user_input, is_valid_chain, plot_location_getter, print_ascii_art, print_version,
};

/// defaults for the user config file
const DEFAULT_PLOT_SIZE: bytesize::ByteSize = bytesize::ByteSize::gib(100);
const DEFAULT_CHAIN: &str = "dev";

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
    let default_plot_loc = plot_location_getter();

    // get user inputs
    let reward_address = get_user_input("Enter your farmer/reward address: ", None, |s| {
        s.parse::<subspace_sdk::PublicKey>()
    })?;

    let default_node_name = whoami::username();
    let node_name = get_user_input(
        &format!("Enter your node name to be identified on the network(defaults to `{default_node_name}`, press enter to use the default): "),
        Some(default_node_name),
        |node_name| {
            if node_name.is_empty() { Ok(node_name) } else { Err("Node name supposed to be not empty") }
        },
    )?;

    let plot_directory = get_user_input(
        &format!(
            "Specify a plot location (press enter to use the default: `{default_plot_loc:?}`): "
        ),
        Some(default_plot_loc),
        |path| {
            let path = PathBuf::from(&path);
            if path.is_dir() {
                Ok(path)
            } else {
                Err("Supplied path should be an existing directory")
            }
        },
    )?;

    let plot_size = get_user_input(
        &format!(
            "Specify a plot size (defaults to `{}`, press enter to use the default): ",
            DEFAULT_PLOT_SIZE
        ),
        Some(DEFAULT_PLOT_SIZE),
        |s| match s.parse::<bytesize::ByteSize>() {
            Ok(ok) if ok >= bytesize::ByteSize::gb(1) => Ok(ok),
            Ok(_) => Err(eyre!("Plot size should be more than 1Gib")),
            Err(err) => Err(eyre!("Failed to parse byte size: {err}")),
        },
    )?;

    let chain = get_user_input(
        &format!(
            "Specify the chain to farm(defaults to `{}`, press enter to use the default): ",
            DEFAULT_CHAIN
        ),
        Some(DEFAULT_CHAIN.to_owned()),
        |chain| {
            if is_valid_chain(&chain) {
                Ok(chain)
            } else {
                Err(eyre!("Unknown chain `{chain}'"))
            }
        },
    )?;

    Ok(Config {
        farmer: FarmerConfig {
            address: reward_address,
            plot_directory,
            plot_size,
            opencl: false,
        },
        node: NodeConfig {
            chain,
            execution: "wasm".to_owned(),
            blocks_pruning: 1024,
            state_pruning: 1024,
            role: subspace_sdk::node::Role::Full,
            name: node_name,
            listen_addresses: vec![],
            rpc_method: subspace_sdk::node::RpcMethods::Auto,
            force_authoring: false,
        },
        chains: ChainConfig {
            dev: "that local node experience".to_owned(),
        },
    })
}
