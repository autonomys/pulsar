//! Crate with DSN shared between sdk farmer and sdk node

#![warn(
    missing_docs,
    clippy::dbg_macro,
    clippy::unwrap_used,
    clippy::disallowed_types,
    unused_features
)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![feature(concat_idents, const_option)]

mod builder;
mod local_provider_record_utils;

pub use builder::*;
use subspace_farmer::piece_cache::PieceCache as FarmerPieceCache;
use tracing::warn;

/// A record provider that uses farmer piece cache underneath
pub type LocalRecordProvider =
    local_provider_record_utils::MaybeLocalRecordProvider<FarmerPieceCache>;
