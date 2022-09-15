use clap::Command;

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

fn main() {
    let command = cli();
    if let Ok(matches) = command.clone().try_get_matches() {
        match matches.subcommand() {
            Some(("init", _)) => {
                println!("hmm");
            }
            Some(("farm", _)) => {
                println!(
                    "Config could not be found. Please run `subspace init` to generate the default"
                )
            }
            _ => unreachable!(), // all commands are defined above
        }
    } else {
        println!("You have entered an invalid command.\n");
        command.clone().print_help().unwrap();
    }
}
