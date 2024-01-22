pub mod common;
#[cfg(all(feature = "core-payments", feature = "executor"))]
mod domains;
mod farmer;
mod node;

#[cfg(all(
    target_arch = "x86_64",
    target_vendor = "unknown",
    target_os = "linux",
    target_env = "gnu"
))]
#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[test]
fn pubkey_parse() {
    "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY".parse::<subspace_sdk::PublicKey>().unwrap();
}
