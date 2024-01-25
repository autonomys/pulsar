pub mod common;
#[cfg(all(feature = "core-payments", feature = "executor"))]
mod domains;
mod farmer;
mod node;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[test]
fn pubkey_parse() {
    "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY".parse::<subspace_sdk::PublicKey>().unwrap();
}
