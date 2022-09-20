use crate::ss58::parse_ss58_reward_address;
use bytesize::ByteSize;
use std::path::Path;

pub(crate) fn print_ascii_art() {
    println!("
 ____             __                                              __  __          __                               __
/\\  _`\\          /\\ \\                                            /\\ \\/\\ \\        /\\ \\__                           /\\ \\
\\ \\,\\L\\_\\  __  __\\ \\ \\____    ____  _____      __      ___     __\\ \\ `\\\\ \\     __\\ \\ ,_\\  __  __  __    ___   _ __\\ \\ \\/'\\
 \\/_\\__ \\ /\\ \\/\\ \\\\ \\ '__`\\  /',__\\/\\ '__`\\  /'__`\\   /'___\\ /'__`\\ \\ , ` \\  /'__`\\ \\ \\/ /\\ \\/\\ \\/\\ \\  / __`\\/\\`'__\\ \\ , <
   /\\ \\L\\ \\ \\ \\_\\ \\\\ \\ \\L\\ \\/\\__, `\\ \\ \\L\\ \\/\\ \\L\\.\\_/\\ \\__//\\  __/\\ \\ \\`\\ \\/\\  __/\\ \\ \\_\\ \\ \\_/ \\_/ \\/\\ \\L\\ \\ \\ \\/ \\ \\ \\\\`\\
   \\ `\\____\\ \\____/ \\ \\_,__/\\/\\____/\\ \\ ,__/\\ \\__/.\\_\\ \\____\\ \\____\\\\ \\_\\ \\_\\ \\____\\\\ \\__\\\\ \\___x___/'\\ \\____/\\ \\_\\  \\ \\_\\ \\_\\
    \\/_____/\\/___/   \\/___/  \\/___/  \\ \\ \\/  \\/__/\\/_/\\/____/\\/____/ \\/_/\\/_/\\/____/ \\/__/ \\/__//__/   \\/___/  \\/_/   \\/_/\\/_/
                                      \\ \\_\\
                                       \\/_/
");
}

pub(crate) fn print_version() {
    let version: &str = env!("CARGO_PKG_VERSION");
    println!("version: {version}");
}

pub(crate) fn get_user_input(
    prompt: &str,
    default_value: Option<&str>,
    condition: fn(input: &str) -> bool,
    error_msg: &str,
) -> String {
    let user_input = loop {
        print!("{prompt}");
        std::io::Write::flush(&mut std::io::stdout()).expect("flush failed!");
        let mut input = String::new();
        if let Err(why) = std::io::stdin().read_line(&mut input) {
            panic!("could not read user input because: {why}");
        }
        let user_input = input.trim().to_string();

        if condition(&user_input) {
            break user_input;
        }
        println!("{error_msg}");
    };

    if let Some(default) = default_value && user_input.is_empty() {
        return default.to_string();
    }
    user_input
}

pub(crate) fn is_valid_hostname(hostname: &str) -> bool {
    hostname.is_ascii()
}

pub(crate) fn is_valid_address(address: &str) -> bool {
    parse_ss58_reward_address(address).is_ok()
}

pub(crate) fn is_valid_location(location: &str) -> bool {
    Path::new(location).is_dir()
}

pub(crate) fn is_valid_size(size: &str) -> bool {
    size.parse::<ByteSize>().is_ok()
}

pub(crate) fn is_valid_chain(chain: &str) -> bool {
    // TODO: instead of a hardcoded list, get the chain names from telemetry
    let chain_list = vec!["gemini-2a", "gemini-1", "testnet", "lamda2513-3", "x-net-1"];
    chain_list.contains(&chain)
}
