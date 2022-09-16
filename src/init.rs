use std::{
    fs::{create_dir, File},
    path::PathBuf,
};

use crate::utils::{get_user_input, print_ascii_art, print_version};

pub(crate) fn init() {
    let (config, config_path) = create_config();
    print_ascii_art();
    print_version();
    write_config(config);
    /*
    Enter your farmer/reward address: WALLET_ADDRESS
    Enter your node name to be identified on the network(defaults to HOSTNAME, press enter to use the default): HOSTNAME
    Specify a sector location (whatever the default was, press enter to use the default):
    Specify a sector size (defaults to 1GB, press enter to use the default): 100GB
    Specify the chain to farm(defaults to `gemini-1`, press enter to use the default): taurus-2

    Configuration has been generated at $HOME/.config/subspace/config
    */

    println!(
        "Configuration has been generated at {}",
        config_path.display()
    );

    println!("Ready for lift off! Run the follow command to begin:");
    println!("'subspace farm'");
}

fn write_config(_config: File) {
    print!("Enter your farmer/reward address: ");
    let _reward_address = get_user_input();

    print!("Enter your node name to be identified on the network(defaults to HOSTNAME, press enter to use the default): ");
    let _hostname = get_user_input();

    print!(
        "Specify a sector location (whatever the default was, press enter to use the default): "
    );
    let _plot_location = get_user_input();

    print!("Specify a sector size (defaults to 1GB, press enter to use the default): ");
    let _plot_size = get_user_input();

    print!("Specify the chain to farm(defaults to `gemini-1`, press enter to use the default): ");
    let _chain = get_user_input();
}

/// Creates a config file at the location
/// - **Linux:** `$HOME/.config/subspace-cli/subspace.cfg`.
/// - **macOS:** `$HOME/Library/Application Support/subspace-cli/subspace.cfg`.
/// - **Windows:** `{FOLDERID_RoamingAppData}/subspace-cli/subspace.cfg`.
pub(crate) fn create_config() -> (File, PathBuf) {
    let config_path = match dirs::config_dir() {
        Some(path) => path,
        None => panic!("couldn't get the default config directory!"),
    };
    let config_path = config_path.join("subspace-cli");
    let _ = create_dir(config_path.clone()); // if folder already exists, ignore the error

    match File::create(config_path.join("subspace.cfg")) {
        Err(why) => panic!("couldn't create the config file because: {}", why),
        Ok(file) => (file, config_path),
    }
}
