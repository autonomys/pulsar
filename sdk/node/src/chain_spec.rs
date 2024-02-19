//! Subspace chain configurations.

use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::num::NonZeroU32;

use hex_literal::hex;
use parity_scale_codec::Encode;
use sc_service::{ChainType, GenericChainSpec, NoExtension};
use sc_subspace_chain_specs::{DEVNET_CHAIN_SPEC, GEMINI_3H_CHAIN_SPEC};
use sc_telemetry::TelemetryEndpoints;
use sdk_utils::chain_spec as utils;
use sdk_utils::chain_spec::{chain_spec_properties, get_public_key_from_seed};
use sp_consensus_subspace::FarmerPublicKey;
use sp_core::crypto::{Ss58Codec, UncheckedFrom};
use sp_domains::storage::RawGenesis;
use sp_domains::{OperatorAllowList, OperatorPublicKey, RuntimeType};
use sp_runtime::{BuildStorage, Percent};
use subspace_core_primitives::PotKey;
use subspace_runtime::{
    AllowAuthoringBy, BalancesConfig, DomainsConfig, EnableRewardsAt, MaxDomainBlockSize,
    MaxDomainBlockWeight, RuntimeConfigsConfig, RuntimeGenesisConfig, SubspaceConfig, SudoConfig,
    SystemConfig, VestingConfig, MILLISECS_PER_BLOCK, WASM_BINARY,
};
use subspace_runtime_primitives::{AccountId, Balance, BlockNumber, SSC};

use crate::domains::evm_chain_spec;
use crate::domains::evm_chain_spec::SpecId;

const SUBSPACE_TELEMETRY_URL: &str = "wss://telemetry.subspace.network/submit/";

/// List of accounts which should receive token grants, amounts are specified in
/// SSC.
const TOKEN_GRANTS: &[(&str, u128)] = &[
    ("5Dns1SVEeDqnbSm2fVUqHJPCvQFXHVsgiw28uMBwmuaoKFYi", 3_000_000),
    ("5DxtHHQL9JGapWCQARYUAWj4yDcwuhg9Hsk5AjhEzuzonVyE", 1_500_000),
    ("5EHhw9xuQNdwieUkNoucq2YcateoMVJQdN8EZtmRy3roQkVK", 133_333),
    ("5C5qYYCQBnanGNPGwgmv6jiR2MxNPrGnWYLPFEyV1Xdy2P3x", 178_889),
    ("5GBWVfJ253YWVPHzWDTos1nzYZpa9TemP7FpQT9RnxaFN6Sz", 350_000),
    ("5F9tEPid88uAuGbjpyegwkrGdkXXtaQ9sGSWEnYrfVCUCsen", 111_111),
    ("5DkJFCv3cTBsH5y1eFT94DXMxQ3EmVzYojEA88o56mmTKnMp", 244_444),
    ("5G23o1yxWgVNQJuL4Y9UaCftAFvLuMPCRe7BCARxCohjoHc9", 311_111),
    ("5GhHwuJoK1b7uUg5oi8qUXxWHdfgzv6P5CQSdJ3ffrnPRgKM", 317_378),
    ("5EqBwtqrCV427xCtTsxnb9X2Qay39pYmKNk9wD9Kd62jLS97", 300_000),
    ("5D9pNnGCiZ9UqhBQn5n71WFVaRLvZ7znsMvcZ7PHno4zsiYa", 600_000),
    ("5DXfPcXUcP4BG8LBSkJDrfFNApxjWySR6ARfgh3v27hdYr5S", 430_000),
    ("5CXSdDJgzRTj54f9raHN2Z5BNPSMa2ETjqCTUmpaw3ECmwm4", 330_000),
    ("5DqKxL7bQregQmUfFgzTMfRKY4DSvA1KgHuurZWYmxYSCmjY", 200_000),
    ("5CfixiS93yTwHQbzzfn8P2tMxhKXdTx7Jam9htsD7XtiMFtn", 27_800),
    ("5FZe9YzXeEXe7sK5xLR8yCmbU8bPJDTZpNpNbToKvSJBUiEo", 18_067),
    ("5FZwEgsvZz1vpeH7UsskmNmTpbfXvAcojjgVfShgbRqgC1nx", 27_800),
];

/// Additional subspace specific genesis parameters.
pub struct GenesisParams {
    enable_rewards_at: EnableRewardsAt<BlockNumber>,
    allow_authoring_by: AllowAuthoringBy,
    pot_slot_iterations: NonZeroU32,
    enable_domains: bool,
    enable_dynamic_cost_of_storage: bool,
    enable_balance_transfers: bool,
    enable_non_root_calls: bool,
    confirmation_depth_k: u32,
}

struct GenesisDomainParams {
    domain_name: String,
    operator_allow_list: OperatorAllowList<AccountId>,
    operator_signing_key: OperatorPublicKey,
}

/// Chain spec type for the subspace
pub type ChainSpec = GenericChainSpec<RuntimeGenesisConfig>;

/// Gemini 3g chain spec
pub fn gemini_3h() -> ChainSpec {
    ChainSpec::from_json_bytes(GEMINI_3H_CHAIN_SPEC.as_bytes()).expect("Always valid")
}

/// Gemini 3g compiled chain spec
pub fn gemini_3h_compiled() -> Result<GenericChainSpec<RuntimeGenesisConfig>, String> {
    // TODO: Migrate once https://github.com/paritytech/polkadot-sdk/issues/2963 is un-broken
    #[allow(deprecated)]
    Ok(GenericChainSpec::from_genesis(
        // Name
        "Subspace Gemini 3h",
        // ID
        "subspace_gemini_3h",
        ChainType::Custom("Subspace Gemini 3h".to_string()),
        || {
            let sudo_account =
                AccountId::from_ss58check("5DNwQTHfARgKoa2NdiUM51ZUow7ve5xG9S2yYdSbVQcnYxBA")
                    .expect("Wrong root account address");

            let mut balances = vec![(sudo_account.clone(), 1_000 * SSC)];
            let vesting_schedules = TOKEN_GRANTS
                .iter()
                .flat_map(|&(account_address, amount)| {
                    let account_id = AccountId::from_ss58check(account_address)
                        .expect("Wrong vesting account address");
                    let amount: Balance = amount * SSC;

                    // TODO: Adjust start block to real value before mainnet launch
                    let start_block = 100_000_000;
                    let one_month_in_blocks =
                        u32::try_from(3600 * 24 * 30 * MILLISECS_PER_BLOCK / 1000)
                            .expect("One month of blocks always fits in u32; qed");

                    // Add balance so it can be locked
                    balances.push((account_id.clone(), amount));

                    [
                        // 1/4 of tokens are released after 1 year.
                        (account_id.clone(), start_block, one_month_in_blocks * 12, 1, amount / 4),
                        // 1/48 of tokens are released every month after that for 3 more years.
                        (
                            account_id,
                            start_block + one_month_in_blocks * 12,
                            one_month_in_blocks,
                            36,
                            amount / 48,
                        ),
                    ]
                })
                .collect::<Vec<_>>();
            subspace_genesis_config(
                SpecId::Gemini,
                sudo_account.clone(),
                balances,
                vesting_schedules,
                GenesisParams {
                    enable_rewards_at: EnableRewardsAt::Manually,
                    allow_authoring_by: AllowAuthoringBy::RootFarmer(
                        FarmerPublicKey::unchecked_from(hex_literal::hex!(
                            "8aecbcf0b404590ddddc01ebacb205a562d12fdb5c2aa6a4035c1a20f23c9515"
                        )),
                    ),
                    // TODO: Adjust once we bench PoT on faster hardware
                    // About 1s on 6.0 GHz Raptor Lake CPU (14900K)
                    pot_slot_iterations: NonZeroU32::new(200_032_000).expect("Not zero; qed"),
                    enable_domains: false,
                    enable_dynamic_cost_of_storage: false,
                    enable_balance_transfers: true,
                    enable_non_root_calls: false,
                    confirmation_depth_k: 100, // TODO: Proper value here
                },
                GenesisDomainParams {
                    domain_name: "nova".to_owned(),
                    operator_allow_list: OperatorAllowList::Operators(BTreeSet::from_iter(vec![
                        sudo_account,
                    ])),
                    operator_signing_key: OperatorPublicKey::unchecked_from(hex!(
                        "aa3b05b4d649666723e099cf3bafc2f2c04160ebe0e16ddc82f72d6ed97c4b6b"
                    )),
                },
            )
        },
        // Bootnodes
        vec![],
        // Telemetry
        Some(
            TelemetryEndpoints::new(vec![(SUBSPACE_TELEMETRY_URL.into(), 1)])
                .map_err(|error| error.to_string())?,
        ),
        // Protocol ID
        Some("subspace-gemini-3h"),
        None,
        // Properties
        Some({
            let mut properties = chain_spec_properties();
            properties.insert(
                "potExternalEntropy".to_string(),
                serde_json::to_value(None::<PotKey>).expect("Serialization is infallible; qed"),
            );
            properties
        }),
        // Extensions
        NoExtension::None,
        // Code
        WASM_BINARY.expect("Wasm binary must be built for Gemini"),
    ))
}

/// Dev net raw configuration
pub fn devnet_config() -> ChainSpec {
    ChainSpec::from_json_bytes(DEVNET_CHAIN_SPEC.as_bytes()).expect("Always valid")
}

/// Dev net compiled configuration
pub fn devnet_config_compiled() -> ChainSpec {
    // TODO: Migrate once https://github.com/paritytech/polkadot-sdk/issues/2963 is un-broken
    #[allow(deprecated)]
    ChainSpec::from_genesis(
        // Name
        "Subspace Dev network",
        // ID
        "subspace_devnet",
        ChainType::Custom("Testnet".to_string()),
        || {
            let sudo_account =
                AccountId::from_ss58check("5CXTmJEusve5ixyJufqHThmy4qUrrm6FyLCR7QfE4bbyMTNC")
                    .expect("Wrong root account address");

            let mut balances = vec![(sudo_account.clone(), 1_000 * SSC)];
            let vesting_schedules = TOKEN_GRANTS
                .iter()
                .flat_map(|&(account_address, amount)| {
                    let account_id = AccountId::from_ss58check(account_address)
                        .expect("Wrong vesting account address");
                    let amount: Balance = amount * SSC;

                    // TODO: Adjust start block to real value before mainnet launch
                    let start_block = 100_000_000;
                    let one_month_in_blocks =
                        u32::try_from(3600 * 24 * 30 * MILLISECS_PER_BLOCK / 1000)
                            .expect("One month of blocks always fits in u32; qed");

                    // Add balance so it can be locked
                    balances.push((account_id.clone(), amount));

                    [
                        // 1/4 of tokens are released after 1 year.
                        (account_id.clone(), start_block, one_month_in_blocks * 12, 1, amount / 4),
                        // 1/48 of tokens are released every month after that for 3 more years.
                        (
                            account_id,
                            start_block + one_month_in_blocks * 12,
                            one_month_in_blocks,
                            36,
                            amount / 48,
                        ),
                    ]
                })
                .collect::<Vec<_>>();
            subspace_genesis_config(
                evm_chain_spec::SpecId::DevNet,
                sudo_account,
                balances,
                vesting_schedules,
                GenesisParams {
                    enable_rewards_at: EnableRewardsAt::Manually,
                    allow_authoring_by: AllowAuthoringBy::FirstFarmer,
                    pot_slot_iterations: NonZeroU32::new(150_000_000).expect("Not zero; qed"),
                    enable_domains: true,
                    enable_dynamic_cost_of_storage: false,
                    enable_balance_transfers: true,
                    enable_non_root_calls: false,
                    confirmation_depth_k: 100, // TODO: Proper value here
                },
                GenesisDomainParams {
                    domain_name: "evm-domain".to_owned(),
                    operator_allow_list: OperatorAllowList::Anyone,
                    operator_signing_key: OperatorPublicKey::unchecked_from(hex!(
                        "aa3b05b4d649666723e099cf3bafc2f2c04160ebe0e16ddc82f72d6ed97c4b6b"
                    )),
                },
            )
        },
        // Bootnodes
        vec![],
        // Telemetry
        Some(
            TelemetryEndpoints::new(vec![(SUBSPACE_TELEMETRY_URL.into(), 1)])
                .expect("Telemetry value is valid"),
        ),
        // Protocol ID
        Some("subspace-devnet"),
        None,
        // Properties
        Some({
            let mut properties = chain_spec_properties();
            properties.insert(
                "potExternalEntropy".to_string(),
                serde_json::to_value(None::<PotKey>).expect("Serialization is not infallible; qed"),
            );
            properties
        }),
        // Extensions
        None,
        // Code
        WASM_BINARY.expect("WASM binary was not build, please build it!"),
    )
}

/// New dev chain spec
pub fn dev_config() -> ChainSpec {
    // TODO: Migrate once https://github.com/paritytech/polkadot-sdk/issues/2963 is un-broken
    #[allow(deprecated)]
    ChainSpec::from_genesis(
        // Name
        "Subspace development",
        // ID
        "subspace_dev",
        ChainType::Development,
        || {
            subspace_genesis_config(
                evm_chain_spec::SpecId::Dev,
                // Sudo account
                utils::get_account_id_from_seed("Alice"),
                // Pre-funded accounts
                vec![
                    (utils::get_account_id_from_seed("Alice"), 1_000 * SSC),
                    (utils::get_account_id_from_seed("Bob"), 1_000 * SSC),
                    (utils::get_account_id_from_seed("Alice//stash"), 1_000 * SSC),
                    (utils::get_account_id_from_seed("Bob//stash"), 1_000 * SSC),
                ],
                vec![],
                GenesisParams {
                    enable_rewards_at: EnableRewardsAt::Manually,
                    allow_authoring_by: AllowAuthoringBy::Anyone,
                    pot_slot_iterations: NonZeroU32::new(100_000_000).expect("Not zero; qed"),
                    enable_domains: true,
                    enable_dynamic_cost_of_storage: false,
                    enable_balance_transfers: true,
                    enable_non_root_calls: true,
                    confirmation_depth_k: 5,
                },
                GenesisDomainParams {
                    domain_name: "evm-domain".to_owned(),
                    operator_allow_list: OperatorAllowList::Anyone,
                    operator_signing_key: get_public_key_from_seed::<OperatorPublicKey>("Alice"),
                },
            )
        },
        // Bootnodes
        vec![],
        // Telemetry
        None,
        // Protocol ID
        None,
        None,
        // Properties
        Some({
            let mut properties = chain_spec_properties();
            properties.insert(
                "potExternalEntropy".to_string(),
                serde_json::to_value(None::<PotKey>).expect("Serialization is not infallible; qed"),
            );
            properties
        }),
        // Extensions
        None,
        // Code
        WASM_BINARY.expect("WASM binary was not build, please build it!"),
    )
}

/// Configure initial storage state for FRAME modules.
fn subspace_genesis_config(
    evm_domain_spec_id: evm_chain_spec::SpecId,
    sudo_account: AccountId,
    balances: Vec<(AccountId, Balance)>,
    // who, start, period, period_count, per_period
    vesting: Vec<(AccountId, BlockNumber, BlockNumber, u32, Balance)>,
    genesis_params: GenesisParams,
    genesis_domain_params: GenesisDomainParams,
) -> RuntimeGenesisConfig {
    let GenesisParams {
        enable_rewards_at,
        allow_authoring_by,
        pot_slot_iterations,
        enable_domains,
        enable_dynamic_cost_of_storage,
        enable_balance_transfers,
        enable_non_root_calls,
        confirmation_depth_k,
    } = genesis_params;

    let raw_genesis_storage = {
        let domain_chain_spec = match evm_domain_spec_id {
            SpecId::Dev => evm_chain_spec::development_config(move || {
                evm_chain_spec::get_testnet_genesis_by_spec_id(evm_domain_spec_id)
            }),
            SpecId::Gemini => evm_chain_spec::gemini_3h_config(move || {
                evm_chain_spec::get_testnet_genesis_by_spec_id(evm_domain_spec_id)
            }),
            SpecId::DevNet => evm_chain_spec::devnet_config(move || {
                evm_chain_spec::get_testnet_genesis_by_spec_id(evm_domain_spec_id)
            }),
        };
        let storage = domain_chain_spec
            .build_storage()
            .expect("Failed to build genesis storage from genesis runtime config");
        let raw_genesis = RawGenesis::from_storage(storage);
        raw_genesis.encode()
    };

    RuntimeGenesisConfig {
        domains: DomainsConfig {
            genesis_domain: Some(sp_domains::GenesisDomain {
                runtime_name: "evm".into(),
                runtime_type: RuntimeType::Evm,
                runtime_version: evm_domain_runtime::VERSION,

                // Domain config, mainly for placeholder the concrete value TBD
                raw_genesis_storage,
                owner_account_id: sudo_account.clone(),
                domain_name: genesis_domain_params.domain_name,
                max_block_size: MaxDomainBlockSize::get(),
                max_block_weight: MaxDomainBlockWeight::get(),
                bundle_slot_probability: (1, 1),
                target_bundles_per_block: 10,
                operator_allow_list: genesis_domain_params.operator_allow_list,
                signing_key: genesis_domain_params.operator_signing_key,
                nomination_tax: Percent::from_percent(5),
                minimum_nominator_stake: 100 * SSC,
            }),
        },
        system: SystemConfig::default(),
        balances: BalancesConfig { balances },
        transaction_payment: Default::default(),
        sudo: SudoConfig {
            // Assign network admin rights.
            key: Some(sudo_account),
        },
        subspace: SubspaceConfig {
            enable_rewards_at,
            allow_authoring_by,
            pot_slot_iterations,
            phantom: PhantomData,
        },
        vesting: VestingConfig { vesting },
        runtime_configs: RuntimeConfigsConfig {
            enable_domains,
            enable_dynamic_cost_of_storage,
            enable_balance_transfers,
            enable_non_root_calls,
            confirmation_depth_k,
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn test_chain_specs() {
        gemini_3h_compiled().expect("Compiled chain spec is okay always");
        gemini_3h();
        devnet_config_compiled();
        devnet_config();
        dev_config();
    }
}
