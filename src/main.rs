use clap::Command;
use utils::create_config;
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
    todo!("initialize the config");
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
