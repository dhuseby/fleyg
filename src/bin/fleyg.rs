#![doc = include_str!("../../README.md")]

use env_logger::Env;
use futures::prelude::*;
use libp2p::{
    development_transport,
    identify::{self, Event as IdentifyEvent},
    identity,
    kad::{
        record::store::MemoryStore, GetClosestPeersError, Kademlia, KademliaConfig, KademliaEvent,
        QueryResult,
    },
    swarm::{keep_alive, NetworkBehaviour, SwarmBuilder, SwarmEvent},
    PeerId,
};
use log::*;
use std::{error::Error, time::Duration};
use structopt::StructOpt;

const BOOTNODES: [&str; 4] = [
    "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    "QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    "QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
];

#[derive(Debug, StructOpt)]
#[structopt(
    name = "fleyg",
    version = "0.1",
    author = "Dave Huseby <dwh@linuxprogrammer.org>",
    about = "libp2p peer tools"
)]
struct Opt {
    /// dial bootstrap peers
    #[structopt(long, short)]
    dial: bool,
}

// our network behavior combines ping and identify
#[derive(NetworkBehaviour)]
struct FleygBehavior {
    keep_alive: keep_alive::Behaviour,
    identify: identify::Behaviour,
    kademlia: Kademlia<MemoryStore>,
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
        let keep_alive = keep_alive::Behaviour::default();
        let identify = {
            let cfg = identify::Config::new("ipfs/0.1.0".into(), local_key.public())
                .with_agent_version("fleyg/0.0.1".into());
            identify::Behaviour::new(cfg)
        };
        let kademlia = {
            let mut cfg = KademliaConfig::default();
            cfg.set_query_timeout(Duration::from_secs(5 * 60));
            let store = MemoryStore::new(local_peer_id);
            let mut behavior = Kademlia::with_config(local_peer_id, store, cfg);
            for peer in &BOOTNODES {
                behavior.add_address(&peer.parse()?, "/dnsaddr/bootstrap.libp2p.io".parse()?);
            }
            for protocol in behavior.protocol_names() {
                info!("Kademlia protocol: {protocol}");
            }
            behavior
        };

        let behavior = FleygBehavior {
            keep_alive,
            identify,
            kademlia,
        };
        SwarmBuilder::with_async_std_executor(transport, behavior, local_peer_id).build()
    };

    // listen on all interfaces
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // bootstrap into the DHT
    //swarm.behaviour_mut().kademlia.bootstrap()?;

    if opt.dial {
        for peer in &BOOTNODES {
            let pid: PeerId = peer.parse()?;
            swarm.dial(pid)?;
            info!("Dialed via peer id {}", pid);
        }
    }

    loop {
        let e = swarm.select_next_some().await;
        match e {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                info!("Connection established to {peer_id}");
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                info!("Connection closed to {peer_id}");
            }
            SwarmEvent::IncomingConnection { send_back_addr, .. } => {
                info!("Incoming connection from {send_back_addr:?}");
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Listening on {address:?}");
            }
            SwarmEvent::Dialing { peer_id, .. } => {
                info!(
                    "Dialing {}",
                    if peer_id.is_some() {
                        peer_id.unwrap().to_base58()
                    } else {
                        "None".into()
                    }
                );
            }
            SwarmEvent::Behaviour(behavior) => match behavior {
                FleygBehaviorEvent::Identify(event) => match event {
                    IdentifyEvent::Received { peer_id, info } => {
                        info!("Identify Received: {peer_id}");
                        info!("\tProtocol: {}", info.protocol_version);
                        info!("\tAgent: {}", info.agent_version);
                        info!("\tAddr: {}", info.observed_addr);
                        info!("\tProtocols:");
                        for sp in &info.protocols {
                            info!("\t\t{}", sp);
                        }

                        // add our observed address
                        swarm.add_external_address(info.observed_addr.clone());
                    }
                    IdentifyEvent::Sent { peer_id } => {
                        info!("Identify Sent: {peer_id}");
                    }
                    IdentifyEvent::Pushed { peer_id } => {
                        info!("Idenfity Pushed: {peer_id}");
                    }
                    IdentifyEvent::Error { peer_id, error } => {
                        info!("Identify Error: {peer_id} - {error}");
                    }
                },
                FleygBehaviorEvent::Kademlia(kad) => match kad {
                    KademliaEvent::InboundRequest { request } => match request {
                        _ => {
                            info!("InboundRequest: {request:?}");
                        }
                    },
                    KademliaEvent::OutboundQueryProgressed { result, .. } => match result {
                        QueryResult::GetClosestPeers(result) => match result {
                            Ok(ok) => {
                                for peer in &ok.peers {
                                    info!("Closest peer: {:#?}", peer);
                                }
                                break;
                            }
                            Err(GetClosestPeersError::Timeout { peers, .. }) => {
                                info!("Query timed out...");
                                for peer in &peers {
                                    info!("Closest peer: {:#?}", peer);
                                }
                                break;
                            }
                        },
                        _ => {}
                    },
                    KademliaEvent::RoutingUpdated { peer, .. } => {
                        info!("Kademlia Routing Updated: {peer:?}");
                    }
                    KademliaEvent::UnroutablePeer { peer } => {
                        info!("Kademlia Unroutable Peer: {peer:?}");
                    }
                    KademliaEvent::RoutablePeer { peer, .. } => {
                        info!("Kademlia Routable Peer: {peer:?}");
                    }
                    KademliaEvent::PendingRoutablePeer { peer, .. } => {
                        info!("Kademlia Pending Routable Peer: {peer:?}");
                    }
                },
                _ => {
                    info!("Swarm::Behaviour: {behavior:?}");
                }
            },
            _ => {
                info!("Swarm: {e:?}");
            }
        }
    }

    Ok(())
}
