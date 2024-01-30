use std::num::NonZeroU8;
use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, ValueEnum};
use futures::prelude::*;
use subspace_sdk::node::{self, Event, Node, RewardsEvent, SubspaceEvent};
use subspace_sdk::{ByteSize, FarmDescription, Farmer, PublicKey};
use tracing_subscriber::prelude::*;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(ValueEnum, Debug, Clone)]
enum Chain {
    Gemini3f,
    Devnet,
    Dev,
}

/// Mini farmer
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Set the chain
    #[arg(value_enum)]
    chain: Chain,
    #[cfg(feature = "executor")]
    /// Run executor with specified domain
    #[arg(short, long)]
    executor: bool,
    /// Address for farming rewards
    #[arg(short, long)]
    reward_address: PublicKey,
    /// Path for all data
    #[arg(short, long)]
    base_path: Option<PathBuf>,
    /// Size of the plot
    #[arg(short, long)]
    plot_size: ByteSize,
    /// Cache size
    #[arg(short, long, default_value_t = ByteSize::gib(1))]
    cache_size: ByteSize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fdlimit::raise_fd_limit();

    #[cfg(tokio_unstable)]
    let registry = tracing_subscriber::registry().with(console_subscriber::spawn());
    #[cfg(not(tokio_unstable))]
    let registry = tracing_subscriber::registry();

    registry
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("info".parse().unwrap()),
        )
        .init();

    let Args {
        chain,
        #[cfg(feature = "executor")]
        executor,
        reward_address,
        base_path,
        plot_size,
        cache_size: _,
    } = Args::parse();
    let (base_path, _tmp_dir) = base_path.map(|x| (x, None)).unwrap_or_else(|| {
        let tmp = tempfile::tempdir().expect("Failed to create temporary directory");
        (tmp.as_ref().to_owned(), Some(tmp))
    });

    let node_dir = base_path.join("node");
    let node = match chain {
        Chain::Gemini3f => Node::gemini_3g().dsn(
            subspace_sdk::node::DsnBuilder::gemini_3g()
                .provider_storage_path(node_dir.join("provider_storage")),
        ),
        Chain::Devnet => Node::devnet().dsn(
            subspace_sdk::node::DsnBuilder::devnet()
                .provider_storage_path(node_dir.join("provider_storage")),
        ),
        Chain::Dev => Node::dev().dsn(
            subspace_sdk::node::DsnBuilder::dev()
                .provider_storage_path(node_dir.join("provider_storage")),
        ),
    }
    .role(node::Role::Authority);

    #[cfg(feature = "executor")]
    let node = if executor {
        node.system_domain(
            node::domains::ConfigBuilder::new()
                .rpc(subspace_sdk::node::RpcBuilder::new().addr("127.0.0.1:9990".parse().unwrap()))
                .role(node::Role::Authority),
        )
    } else {
        node
    };

    let node = node
        .build(
            &node_dir,
            match chain {
                Chain::Gemini3f => node::chain_spec::gemini_3g(),
                Chain::Devnet => node::chain_spec::devnet_config(),
                Chain::Dev => node::chain_spec::dev_config(),
            },
        )
        .await?;

    let sync = if !matches!(chain, Chain::Dev) {
        futures::future::Either::Left(node.sync())
    } else {
        futures::future::Either::Right(futures::future::ok(()))
    };

    tokio::select! {
        result = sync => result?,
        _ = tokio::signal::ctrl_c() => {
            tracing::error!("Exitting...");
            return node.close().await.context("Failed to close node")
        }
    }
    tracing::error!("Node was synced!");

    let farmer = Farmer::builder()
        .build(
            reward_address,
            &node,
            &[FarmDescription::new(base_path.join("plot"), plot_size)],
            NonZeroU8::new(1).expect("static value should not fail; qed"),
        )
        .await?;

    tokio::spawn({
        let initial_plotting =
            farmer.iter_farms().await.next().unwrap().subscribe_initial_plotting_progress().await;
        async move {
            initial_plotting
                .for_each(|progress| async move {
                    tracing::error!(?progress, "Plotting!");
                })
                .await;
            tracing::error!("Finished initial plotting!");
        }
    });

    let rewards_sub = {
        let node = &node;

        async move {
            let mut new_blocks = node.subscribe_finalized_heads().await?;
            while let Some(header) = new_blocks.next().await {
                let events = node.get_events(Some(header.hash)).await?;

                for event in events {
                    match event {
                        Event::Rewards(
                            RewardsEvent::VoteReward { reward, voter: author }
                            | RewardsEvent::BlockReward { reward, block_author: author },
                        ) if author == reward_address.into() =>
                            tracing::error!(%reward, "Received a reward!"),
                        Event::Subspace(SubspaceEvent::FarmerVote {
                            reward_address: author,
                            height: block_number,
                            ..
                        }) if author == reward_address.into() =>
                            tracing::error!(block_number, "Vote counted for block"),
                        _ => (),
                    };
                }

                if let Some(pre_digest) = header.pre_digest {
                    if pre_digest.solution().reward_address == reward_address {
                        tracing::error!("We authored a block");
                    }
                }
            }

            anyhow::Ok(())
        }
    };

    tokio::select! {
        _ = rewards_sub => {},
        _ = tokio::signal::ctrl_c() => {
            tracing::error!("Exitting...");
        }
    }

    node.close().await.context("Failed to close node")?;
    farmer.close().await.context("Failed to close farmer")?;

    Ok(())
}
