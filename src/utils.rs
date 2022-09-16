use std::fs::{create_dir, File};

/// Creates a config file at the location
/// - **Linux:** `$HOME/.config/subspace-cli/subspace.cfg`.
/// - **macOS:** `$HOME/Library/Application Support/subspace-cli/subspace.cfg`.
/// - **Windows:** `{FOLDERID_RoamingAppData}/subspace-cli/subspace.cfg`.
pub(crate) fn create_config() -> File {
    let config_path = match dirs::config_dir() {
        Some(path) => path,
        None => panic!("couldn't get the default config directory!"),
    };
    let config_path = config_path.join("subspace-cli");
    let _ = create_dir(config_path.clone()); // if folder already exists, ignore the error

    match File::create(config_path.join("subspace.cfg")) {
        Err(why) => panic!("couldn't create the config file because: {}", why),
        Ok(file) => file,
    }
}
