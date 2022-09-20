use crate::utils::{
    get_user_input, is_valid_address, is_valid_chain, is_valid_hostname, is_valid_location,
    is_valid_size, plot_location_getter, print_ascii_art, print_version,
};
use std::{
    fs::{create_dir, File},
    io::Write,
    path::PathBuf,
};

const DEFAULT_PLOT_SIZE: &str = "1GB";
const DEFAULT_HOSTNAME: &str = "HOSTNAME";
const DEFAULT_CHAIN: &str = "gemini-2a";

pub(crate) fn init() {
    let (config, config_path) = create_config();
    print_ascii_art();
    print_version();
    println!();
    println!("Configuration creation process has started...");
    write_config(config);

    println!(
        "Configuration has been generated at {}",
        config_path.display()
    );

    println!("Ready for lift off! Run the follow command to begin:");
    println!("'subspace farm'");
}

// TODO: validate user inputs
// TODO: use the default values if user pressed enter
fn write_config(mut config: File) {
    let default_plot_loc = plot_location_getter();

    // get user inputs
    let reward_address = get_user_input(
        "Enter your farmer/reward address: ",
        None,
        is_valid_address,
        "Reward address is not in the correct format! Please enter a valid address...",
    );

    let hostname = get_user_input(
        &format!("Enter your node name to be identified on the network(defaults to `{}`, press enter to use the default): ", DEFAULT_HOSTNAME),
        Some(DEFAULT_HOSTNAME),
        is_valid_hostname,
        "hostname includes non-ascii characters! Please enter a valid hostname");

    let plot_location = get_user_input(
        &format!(
            "Specify a sector location (press enter to use the default: `{}`): ",
            default_plot_loc.display()
        ),
        default_plot_loc.to_str(),
        is_valid_location,
        "supplied directory does not exist! Please enter a valid path...",
    );

    let plot_size = get_user_input(
        &format!(
            "Specify a sector size (defaults to `{}`, press enter to use the default): ",
            DEFAULT_PLOT_SIZE
        ),
        Some(DEFAULT_PLOT_SIZE),
        is_valid_size,
        "could not parse the value! Please enter a valid size...",
    );

    let chain = get_user_input(
        &format!(
            "Specify the chain to farm(defaults to `{}`, press enter to use the default): ",
            DEFAULT_CHAIN
        ),
        Some(DEFAULT_CHAIN),
        is_valid_chain,
        "given chain is not recognized! Please enter a valid chain...",
    );

    let config_text = construct_config(
        &reward_address,
        &plot_location,
        &plot_size,
        &chain,
        &hostname,
    );

    // write to config
    if let Err(why) = config.write(config_text.as_bytes()) {
        panic!("could not write to config file, because: {why}");
    }
}

/// Creates a config file at the location
/// - **Linux:** `$HOME/.config/subspace-cli/settings.toml`.
/// - **macOS:** `$HOME/Library/Application Support/subspace-cli/settings.toml`.
/// - **Windows:** `{FOLDERID_RoamingAppData}/subspace-cli/settings.toml`.
pub(crate) fn create_config() -> (File, PathBuf) {
    let config_path = match dirs::config_dir() {
        Some(path) => path,
        None => panic!("couldn't get the default config directory!"),
    };
    let config_path = config_path.join("subspace-cli");
    let _ = create_dir(config_path.clone()); // if folder already exists, ignore the error

    match File::create(config_path.join("settings.toml")) {
        Err(why) => panic!("couldn't create the config file because: {}", why),
        Ok(file) => (file, config_path),
    }
}

fn construct_config(
    reward_address: &str,
    plot_location: &str,
    plot_size: &str,
    chain: &str,
    hostname: &str,
) -> String {
    format!(
        "[farmer]
address = \"{}\"
sector_directory = \"{}\"
sector_size = \"{}\"
opencl = false

[node]
chain = \"{}\"
execution = \"wasm\"
blocks-pruning = 1024
state-pruning = 1024
validator = true
name = \"{}\"
port = 30333
unsafe-ws-external = true # not sure we need this

[chains]
gemini-1 = \"rpc://1212312\"
gemini-2= \"rpc://\"
leo-3 = \"myown-network\"
dev = \"that local node experience\"
",
        reward_address, plot_location, plot_size, chain, hostname
    )
}
