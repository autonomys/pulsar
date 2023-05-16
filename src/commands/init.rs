use std::io::{BufRead, Write};
use std::str::FromStr;

use color_eyre::eyre::{eyre, Context, Error, Result};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{cursor, execute};
use rand::prelude::IteratorRandom;
use sp_core::Pair;
use strum::IntoEnumIterator;
use subspace_sdk::PublicKey;
use zeroize::Zeroizing;

use crate::config::{
    create_config, AdvancedFarmerSettings, AdvancedNodeSettings, ChainConfig, Config, FarmerConfig,
    NodeConfig, DEFAULT_PLOT_SIZE,
};
use crate::utils::{
    directory_parser, get_user_input, node_directory_getter, node_name_parser,
    plot_directory_getter, print_ascii_art, print_run_executable_command, print_version,
    reward_address_parser, size_parser, yes_or_no_parser,
};

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
        .write_all(toml::to_string_pretty(&config).wrap_err("Failed to write config")?.as_ref())
        .wrap_err("Failed to write config")?;

    println!("Configuration has been generated at {}", config_path.display());

    println!("Ready for lift off! Run the follow command to begin:");
    print_run_executable_command();

    Ok(())
}

/// gets the necessary information from user, and writes them to the given
/// configuration file
fn get_config_from_user_inputs() -> Result<Config> {
    // check if user has an existing reward address
    let reward_address_exist = get_user_input(
        "Do you have an existing farmer/reward address? [y/n]: ",
        None,
        yes_or_no_parser,
    )?;

    let reward_address = generate_or_get_reward_address(reward_address_exist)
        .context("reward address creation failed")?;

    // get node name
    let default_node_name = whoami::username();
    let node_name = get_user_input(
        &format!(
            "Enter your node name to be identified on the network(defaults to \
             `{default_node_name}`, press enter to use the default): "
        ),
        (default_node_name != "root").then_some(default_node_name),
        node_name_parser,
    )?;

    // get plot directory
    let default_plot_loc = plot_directory_getter();
    let plot_directory = get_user_input(
        &format!(
            "Specify a path for storing plot files (press enter to use the default: \
             `{default_plot_loc:?}`): ",
        ),
        Some(default_plot_loc),
        directory_parser,
    )?;

    let default_node_loc = node_directory_getter();
    let node_directory = get_user_input(
        &format!(
            "Specify a path for storing node files (press enter to use the default: \
             `{default_node_loc:?}`): ",
        ),
        Some(default_node_loc),
        directory_parser,
    )?;

    // get plot size
    let plot_size = get_user_input(
        &format!(
            "Specify a plot size (defaults to `{DEFAULT_PLOT_SIZE}`, press enter to use the \
             default): "
        ),
        Some(DEFAULT_PLOT_SIZE),
        size_parser,
    )?;

    // get chain
    let default_chain = ChainConfig::Gemini3d;
    let chain = get_user_input(
        &format!(
            "Specify the chain to farm. Available options are: {:?}. \n Defaults to \
             `{default_chain:?}`, press enter to use the default:",
            ChainConfig::iter().collect::<Vec<_>>()
        ),
        Some(default_chain),
        ChainConfig::from_str,
    )?;

    let farmer_config = FarmerConfig {
        plot_size,
        plot_directory,
        reward_address,
        advanced: AdvancedFarmerSettings::default(),
    };
    let node_config = NodeConfig {
        name: node_name,
        directory: node_directory,
        advanced: AdvancedNodeSettings::default(),
    };

    Ok(Config { farmer: farmer_config, node: node_config, chain })
}

fn generate_or_get_reward_address(reward_address_exist: bool) -> Result<PublicKey> {
    if reward_address_exist {
        return get_user_input("Enter your farmer/reward address: ", None, reward_address_parser);
    }

    let wants_new_key = get_user_input(
        "Do you want to create a new farmer/reward key? [y/n]: ",
        None,
        yes_or_no_parser,
    )?;

    if !wants_new_key {
        return Err(eyre!("New key creation was not confirmed"));
    }

    // generate new mnemonic and key pair
    let (pair, phrase, seed): (
        sp_core::sr25519::Pair,
        String,
        <sp_core::sr25519::Pair as Pair>::Seed,
    ) = Pair::generate_with_phrase(None);
    let _seed = Zeroizing::new(seed);
    let phrase = Zeroizing::new(phrase);
    let words: Vec<&str> = phrase.split_whitespace().collect();

    println!(
        "IMPORTANT NOTICE: The mnemonic displayed below is crucial to regain access to your \
         account in case you forget your credentials. It's highly recommended to store it in a \
         secure and retrievable location. Failure to do so may result in permanent loss of access \
         to your account.\n"
    );
    println!(
        "Please press 'Enter' after you've securely stored the mnemonic. Once you press 'Enter', \
         the mnemonic will no longer be visible in this interface for security reasons.\n"
    );
    // saving position, since we will later clear the mnemonic
    println!("Here is your mnemonic:");
    execute!(std::io::stdout(), cursor::SavePosition).context("save position failed")?;
    println!("{}", phrase.as_str());
    std::io::stdin().lock().lines().next();

    // clear the mnemonic
    execute!(std::io::stdout(), cursor::RestorePosition).context("restore cursor failed")?;
    execute!(std::io::stdout(), Clear(ClearType::FromCursorDown))
        .context("clear mnemonic failed")?;

    println!("...redacted...");

    // User has to provide 3 randomly selected words from the mnemonic
    let mut rng = rand::thread_rng();
    let word_indexes: Vec<usize> = (0..words.len()).choose_multiple(&mut rng, 3);

    for index in &word_indexes {
        loop {
            let word = get_user_input(
                &format!("Enter the {}th word in the mnemonic: ", index + 1),
                None,
                |input| Ok::<String, Error>(input.to_owned()),
            )?;

            if word == words[*index] {
                break;
            } else {
                println!("incorrect word, please try again.")
            }
        }
    }

    // print the public key and return it
    println!("Your new public key is: {}", pair.public());
    let public_key_array = pair.public().0;
    Ok(public_key_array.into())
}
