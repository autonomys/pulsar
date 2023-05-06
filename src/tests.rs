use std::str::FromStr;

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use subspace_sdk::ByteSize;

use crate::config::ChainConfig;
use crate::summary::*;
use crate::utils::{
    apply_extra_options, cache_directory_getter, custom_log_dir, directory_parser,
    node_directory_getter, node_name_parser, plot_directory_getter, reward_address_parser,
    size_parser, yes_or_no_parser,
};

async fn update_summary_file_randomly(summary_file: SummaryFile) {
    let mut rng = SmallRng::from_entropy();

    for _ in 0..5 {
        let update_fields = SummaryUpdateFields {
            is_plotting_finished: false,
            new_authored_count: rng.gen_range(1..10),
            new_vote_count: rng.gen_range(1..10),
            new_reward: Rewards(rng.gen_range(1..1000)),
            new_parsed_blocks: rng.gen_range(1..100),
        };
        let result = summary_file.update(update_fields).await;
        assert!(result.is_ok(), "Failed to update summary file");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn summary_file_integration() {
    // this test is mainly for CI, in which, summary file won't exist
    // if there is a summary file (user env), we don't want to modify the existing
    // summary file for test
    if SummaryFile::new(None).await.is_ok() {
        return;
    }

    // create summary file
    let plot_size = ByteSize::gb(1);
    let summary_file = SummaryFile::new(Some(plot_size)).await;
    assert!(summary_file.is_ok(), "Failed to create summary file");

    let summary_file = summary_file.unwrap();

    // sequential update trial
    let update_fields = SummaryUpdateFields {
        is_plotting_finished: true,
        new_authored_count: 11,
        new_vote_count: 11,
        new_reward: Rewards(1001),
        new_parsed_blocks: 101,
    };
    let update_result = summary_file.update(update_fields).await;
    assert!(update_result.is_ok(), "Failed to update summary file");

    // create two concurrent tasks, they try to write to summary file 5 times each
    let task1 = tokio::spawn(update_summary_file_randomly(summary_file.clone()));
    let task2 = tokio::spawn(update_summary_file_randomly(summary_file.clone()));

    // Wait for both tasks to complete concurrently
    let (result1, result2) = tokio::join!(task1, task2);

    assert!(result1.is_ok(), "Task 1 encountered an error: {:?}", result1.unwrap_err());
    assert!(result2.is_ok(), "Task 2 encountered an error: {:?}", result2.unwrap_err());

    // parse the summary after updates
    let parse_result = summary_file.parse().await;
    assert!(parse_result.is_ok(), "Failed to parse the summary file after updates",);

    // Clean up the summary file
    assert!(delete_summary().is_ok(), "summary deletion failed");
}

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
fn directory_checker() {
    assert!(directory_parser("./").is_ok());
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
