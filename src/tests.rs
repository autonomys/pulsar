use std::str::FromStr;

use crate::config::ChainConfig;
use crate::utils::{
    apply_extra_options, cache_directory_getter, custom_log_dir, node_directory_getter,
    node_name_parser, plot_directory_getter, plot_directory_parser, reward_address_parser,
    size_parser, yes_or_no_parser,
};

#[test]
fn extra_options() {
    let cargo_toml = toml::toml! {
        name = "toml"

        [package]
        version = "0.4.5"
        authors = ["Alex Crichton <alex@alexcrichton.com>"]
    };
    let extra = toml::toml! {
        name = "toml-edit"
        option = true

        [package]
        version = "0.4.6"
        badges = ["travis-ci"]
    };
    let result = toml::toml! {
        name = "toml-edit"
        option = true

        [package]
        authors = ["Alex Crichton <alex@alexcrichton.com>"]
        version = "0.4.6"
        badges = ["travis-ci"]
    };

    assert_eq!(apply_extra_options(&cargo_toml, extra).unwrap(), result);
}

#[test]
fn yes_no_checker() {
    assert!(yes_or_no_parser("yas").is_err());
    assert!(yes_or_no_parser("yess").is_err());
    assert!(yes_or_no_parser("y").is_ok());
}

#[test]
fn plot_directory_checker() {
    assert!(plot_directory_parser("some-weird-location-that-does-not-exist").is_err());
    assert!(plot_directory_parser("some/weird/location/that/does/not/exist").is_err());
    assert!(plot_directory_parser("./").is_ok());
}

#[test]
fn node_name_checker() {
    assert!(node_name_parser("     ").is_err());
    assert!(node_name_parser("root ").is_err());
    assert!(node_name_parser("ゴゴゴゴ yare yare daze").is_ok());
}

#[test]
fn reward_address_checker() {
    // below address is randomly generated via metamask and then deleted
    assert!(reward_address_parser("5FWr7j9DW4uy7K1JLmFN2R3eoae35PFDUfW7G42ARpBEUaN7").is_ok());
    assert!(reward_address_parser("sdjhfskjfhdksjhfsfhskjskdjhfdsfjhk").is_err());
}

#[test]
fn size_checker() {
    assert!(size_parser("800MB").is_ok());
    assert!(size_parser("103gjie").is_err());
    assert!(size_parser("12GB").is_ok());
}

#[test]
fn chain_checker() {
    assert!(ChainConfig::from_str("gemini3d").is_ok());
    assert!(ChainConfig::from_str("devv").is_err());
}

#[test]
fn plot_directory_tester() {
    let plot_path = plot_directory_getter();

    #[cfg(target_os = "macos")]
    assert!(plot_path.ends_with("Library/Application Support/subspace-cli/plots"));

    #[cfg(target_os = "linux")]
    assert!(plot_path.ends_with(".local/share/subspace-cli/plots"));

    #[cfg(target_os = "windows")]
    assert!(plot_path.ends_with("AppData/Roaming/subspace-cli/plots"));
}

#[test]
fn cache_directory_tester() {
    let cache_path = cache_directory_getter();

    #[cfg(target_os = "macos")]
    assert!(cache_path.ends_with("Library/Application Support/subspace-cli/cache"));

    #[cfg(target_os = "linux")]
    assert!(cache_path.ends_with(".local/share/subspace-cli/cache"));

    #[cfg(target_os = "windows")]
    assert!(cache_path.ends_with("AppData/Roaming/subspace-cli/cache"));
}

#[test]
fn node_directory_tester() {
    let node_path = node_directory_getter();

    #[cfg(target_os = "macos")]
    assert!(node_path.ends_with("Library/Application Support/subspace-cli/node"));

    #[cfg(target_os = "linux")]
    assert!(node_path.ends_with(".local/share/subspace-cli/node"));

    #[cfg(target_os = "windows")]
    assert!(node_path.ends_with("AppData/Roaming/subspace-cli/node"));
}

#[test]
fn custom_log_dir_test() {
    let log_path = custom_log_dir();

    #[cfg(target_os = "macos")]
    assert!(log_path.ends_with("Library/Logs/subspace-cli"));

    #[cfg(target_os = "linux")]
    assert!(log_path.ends_with(".local/share/subspace-cli/logs"));

    #[cfg(target_os = "windows")]
    assert!(log_path.ends_with("AppData/Local/subspace-cli/logs"));
}
