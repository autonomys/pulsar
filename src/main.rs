use clap::Command;
use utils::{create_config, print_ascii_art, print_version};
mod utils;

fn cli() -> Command<'static> {
    Command::new("subspace")
        .about("Subspace CLI interface")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("init").about("initializes the config file required for the farming"),
        )
        .subcommand(
            Command::new("farm")
                .about("starting the farming process (along with node in the background)"),
        )
}

fn config() {
    let _file = create_config();
    print_ascii_art();
    print_version();

    /*
    Enter your farmer/reward address: WALLET_ADDRESS
    Enter your node name to be identified on the network(defaults to HOSTNAME): HOSTNAME
    Specify a sector location (whatever the default was):
    Specify a sector size (defaults to 1GB): 100GB
    Specify the chain to farm(defaults to `gemini-1`): taurus-2

    Configuration has been generated at $HOME/.config/subspace/config
    */

    println!("Ready for lift off! Run the follow command to begin:");
    println!("'subspace farm'");
}

fn main() {
    let command = cli();
    let matches = command.get_matches();
    match matches.subcommand() {
        Some(("init", _)) => {
            config();
        }
        Some(("farm", _)) => {
            println!(
                "Config could not be found. Please run `subspace init` to generate the default"
            )
        }
        _ => unreachable!(), // all commands are defined above
    }
}
