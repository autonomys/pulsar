//! System domain chain specs

use std::str::FromStr;

use evm_domain_runtime::{
    AccountId, BalancesConfig, EVMChainIdConfig, EVMConfig, Precompiles, RuntimeGenesisConfig,
    SudoConfig, SystemConfig, WASM_BINARY,
};
use hex_literal::hex;
use sc_service::{ChainSpec as _, ChainType, GenericChainSpec};
use sdk_utils::chain_spec::chain_spec_properties;
use sp_domains::storage::RawGenesis;
use subspace_runtime_primitives::SSC;

/// Chain spec type for the system domain
pub type ChainSpec = GenericChainSpec<RuntimeGenesisConfig>;

#[derive(Copy, Clone)]
pub enum SpecId {
    Dev,
    Gemini,
    DevNet,
}

pub fn create_domain_spec(chain_id: &str, raw_genesis: RawGenesis) -> Result<ChainSpec, String> {
    // The value of the `RuntimeGenesisConfig` doesn't matter since it will be
    // overwritten later
    let constructor = RuntimeGenesisConfig::default;
    let mut chain_spec = match chain_id {
        "dev" => development_config(constructor),
        "gemini-3g" => gemini_3h_config(constructor),
        "devnet" => devnet_config(constructor),
        path => ChainSpec::from_json_file(std::path::PathBuf::from(path))?,
    };

    chain_spec.set_storage(raw_genesis.into_storage());

    Ok(chain_spec)
}

/// Development keys that will be injected automatically on polkadotjs apps
fn get_dev_accounts() -> Vec<AccountId> {
    vec![
        // Alith key
        AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac")),
        // Baltathar key
        AccountId::from(hex!("3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0")),
        // Charleth key
        AccountId::from(hex!("798d4Ba9baf0064Ec19eB4F0a1a45785ae9D6DFc")),
        // Dorothy
        AccountId::from(hex!("773539d4Ac0e786233D90A233654ccEE26a613D9")),
    ]
}

pub fn get_testnet_genesis_by_spec_id(spec_id: SpecId) -> RuntimeGenesisConfig {
    match spec_id {
        SpecId::Dev => {
            let accounts = get_dev_accounts();
            testnet_genesis(
                accounts.clone(),
                // Alith is Sudo
                Some(accounts[0]),
            )
        }
        SpecId::Gemini => {
            let sudo_account = AccountId::from_str("f31e60022e290708c17d6997c34de6a30d09438f")
                .expect("Invalid Sudo account");
            testnet_genesis(
                vec![
                    // Sudo account
                    sudo_account,
                ],
                Some(sudo_account),
            )
        }
        SpecId::DevNet => {
            let sudo_account = AccountId::from_str("b66a91845249464309fad766fd0ece8144547736")
                .expect("Invalid Sudo account");
            testnet_genesis(
                vec![
                    // Sudo account
                    sudo_account,
                ],
                Some(sudo_account),
            )
        }
    }
}

/// Development config
pub fn development_config<F: Fn() -> RuntimeGenesisConfig + 'static + Send + Sync>(
    constructor: F,
) -> ChainSpec {
    // TODO: Migrate once https://github.com/paritytech/polkadot-sdk/issues/2963 is un-broken
    #[allow(deprecated)]
    ChainSpec::from_genesis(
        // Name
        "Development",
        // ID
        "evm_domain_dev",
        ChainType::Development,
        constructor,
        vec![],
        None,
        None,
        None,
        Some(chain_spec_properties()),
        None,
        // Code
        WASM_BINARY.expect("WASM binary was not build, please build it!"),
    )
}

/// Gemini 3g config
pub fn gemini_3h_config<F: Fn() -> RuntimeGenesisConfig + 'static + Send + Sync>(
    constructor: F,
) -> ChainSpec {
    // TODO: Migrate once https://github.com/paritytech/polkadot-sdk/issues/2963 is un-broken
    #[allow(deprecated)]
    ChainSpec::from_genesis(
        // Name
        "Subspace Gemini 3g EVM Domain",
        // ID
        "subspace_gemini_3h_evm_domain",
        ChainType::Live,
        constructor,
        // Bootnodes
        vec![],
        // Telemetry
        None,
        // Protocol ID
        Some("subspace-gemini-3g-evm-domain"),
        None,
        // Properties
        Some(chain_spec_properties()),
        // Extensions
        None,
        // Code
        WASM_BINARY.expect("WASM binary was not build, please build it!"),
    )
}

pub fn devnet_config<F: Fn() -> RuntimeGenesisConfig + 'static + Send + Sync>(
    constructor: F,
) -> ChainSpec {
    // TODO: Migrate once https://github.com/paritytech/polkadot-sdk/issues/2963 is un-broken
    #[allow(deprecated)]
    ChainSpec::from_genesis(
        // Name
        "Subspace Devnet EVM Domain",
        // ID
        "subspace_devnet_evm_domain",
        ChainType::Custom("Testnet".to_string()),
        constructor,
        // Bootnodes
        vec![],
        // Telemetry
        None,
        // Protocol ID
        Some("subspace-devnet-evm-domain"),
        None,
        // Properties
        Some(chain_spec_properties()),
        // Extensions
        None,
        // Code
        WASM_BINARY.expect("WASM binary was not build, please build it!"),
    )
}

fn testnet_genesis(
    endowed_accounts: Vec<AccountId>,
    maybe_sudo_account: Option<AccountId>,
) -> RuntimeGenesisConfig {
    // This is the simplest bytecode to revert without returning any data.
    // We will pre-deploy it under all of our precompiles to ensure they can be
    // called from within contracts.
    // (PUSH1 0x00 PUSH1 0x00 REVERT)
    let revert_bytecode = vec![0x60, 0x00, 0x60, 0x00, 0xFD];

    RuntimeGenesisConfig {
        system: SystemConfig::default(),
        sudo: SudoConfig { key: maybe_sudo_account },
        transaction_payment: Default::default(),
        balances: BalancesConfig {
            balances: endowed_accounts.iter().cloned().map(|k| (k, 1_000_000 * SSC)).collect(),
        },
        // this is set to default and chain_id will be set into genesis during the domain
        // instantiation on Consensus runtime.
        evm_chain_id: EVMChainIdConfig::default(),
        evm: EVMConfig {
            // We need _some_ code inserted at the precompile address so that
            // the evm will actually call the address.
            accounts: Precompiles::used_addresses()
                .into_iter()
                .map(|addr| {
                    (
                        addr,
                        fp_evm::GenesisAccount {
                            nonce: Default::default(),
                            balance: Default::default(),
                            storage: Default::default(),
                            code: revert_bytecode.clone(),
                        },
                    )
                })
                .collect(),
            ..Default::default()
        },
        ..Default::default()
    }
}
