#![doc = include_str!("../../README.md")]

use env_logger::Env;
use futures::prelude::*;
use libp2p::{
    development_transport, identify, identity,
    swarm::{SwarmBuilder, SwarmEvent},
    Multiaddr, PeerId,
};
use log::*;
use std::error::Error;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "ident",
    version = "0.1",
    author = "Dave Huseby <dwh@linuxprogrammer.org>",
    about = "query a peer for their identify info"
)]
struct Opt {
    /// peer to dial
    #[structopt(long, short)]
    peer: Option<PeerId>,

    /// addr to dial
    #[structopt(long, short)]
    addr: Option<Multiaddr>,
}

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // set up logger
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    // parse the command line arguments
    let opt = Opt::from_args();

    // create a random peer id
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    info!("Local peer id: {}", local_peer_id);

    // set up tcp transport
    let transport = development_transport(local_key.clone()).await?;

    // build the swarm
    let mut swarm = {
        // set up the identify behavior
        let identify = {
            let cfg = identify::Config::new("ipfs/0.1.0".into(), local_key.public())
                .with_agent_version("ident/0.0.1".into());
            identify::Behaviour::new(cfg)
        };

        SwarmBuilder::with_async_std_executor(transport, identify, local_peer_id).build()
    };

    if let Some(addr) = opt.addr {
        swarm.dial(addr.clone())?;
        info!("Dialed via addr {}", addr);
    } else if let Some(peer) = opt.peer {
        swarm.dial(peer.clone())?;
        info!("Dialed via peer {}", peer);
    }

    loop {
        if let SwarmEvent::Behaviour(event) = swarm.select_next_some().await {
            use identify::Event::*;
            match event {
                Received { peer_id, info } => {
                    info!("Identify Received: {peer_id}");
                    info!("\tProtocol: {}", info.protocol_version);
                    info!("\tAgent: {}", info.agent_version);
                    info!("\tAddr: {}", info.observed_addr);
                    info!("\tProtocols:");
                    for sp in &info.protocols {
                        info!("\t\t{}", sp);
                    }
                    //break;
                }
                Sent { peer_id } => {
                    info!("Identify Sent: {peer_id}");
                }
                Pushed { peer_id, info } => {
                    info!("Identify Pushed: {peer_id}");
                    info!("\tProtocol: {}", info.protocol_version);
                    info!("\tAgent: {}", info.agent_version);
                    info!("\tAddr: {}", info.observed_addr);
                    info!("\tProtocols:");
                    for sp in &info.protocols {
                        info!("\t\t{}", sp);
                    }
                }
                Error { peer_id, error } => {
                    info!("Identify Error: {peer_id} - {error}");
                    //break;
                }
            }
        }
    }

    //Ok(())
}
