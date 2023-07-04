use env_logger::Env;
use futures::prelude::*;
use libp2p::{
    core::upgrade,
    identify, identity, noise, ping,
    swarm::{keep_alive, NetworkBehaviour, SwarmBuilder, SwarmEvent},
    tcp, yamux, Multiaddr, Transport,
};
use log::*;
use std::error::Error;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "fleyg",
    version = "0.1",
    author = "Dave Huseby <dwh@linuxprogrammer.org>",
    about = "libp2p peer tools"
)]
struct Opt {
    /// peer to dial
    #[structopt(long, short)]
    peer: Vec<Multiaddr>,
}

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // set up logger
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    // parse the command line arguments
    let opt = Opt::from_args();

    // create a random peer id
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = local_key.public().to_peer_id();
    info!("Local peer id: {}", local_peer_id);

    // set up tcp transport
    let transport = tcp::async_io::Transport::new(tcp::Config::default().nodelay(true))
        .upgrade(upgrade::Version::V1Lazy)
        .authenticate(noise::Config::new(&local_key).expect("signing libp2p-noise static keypair"))
        .multiplex(yamux::Config::default())
        .timeout(std::time::Duration::from_secs(30))
        .boxed();

    // our network behavior combines ping and identify
    #[derive(NetworkBehaviour)]
    #[behaviour(to_swarm = "FleygBehaviorEvent")]
    struct FleygBehavior {
        keep_alive: keep_alive::Behaviour,
        ping: ping::Behaviour,
        identify: identify::Behaviour,
    }

    #[allow(clippy::large_enum_variant)]
    enum FleygBehaviorEvent {
        KeepAlive,
        Ping(ping::Event),
        Identify(identify::Event),
    }

    impl From<void::Void> for FleygBehaviorEvent {
        fn from(_event: void::Void) -> Self {
            FleygBehaviorEvent::KeepAlive
        }
    }

    impl From<ping::Event> for FleygBehaviorEvent {
        fn from(event: ping::Event) -> Self {
            FleygBehaviorEvent::Ping(event)
        }
    }

    impl From<identify::Event> for FleygBehaviorEvent {
        fn from(event: identify::Event) -> Self {
            FleygBehaviorEvent::Identify(event)
        }
    }

    // build the swarm
    let mut swarm = {
        let keep_alive = keep_alive::Behaviour::default();
        let ping = ping::Behaviour::default();
        let identify = identify::Behaviour::new(identify::Config::new(
            "/fleyg/0.1".into(),
            local_key.public(),
        ));
        let behavior = FleygBehavior {
            keep_alive,
            ping,
            identify,
        };
        SwarmBuilder::with_async_std_executor(transport, behavior, local_peer_id).build()
    };

    // dial the specified peers
    for peer in &opt.peer {
        swarm.dial(peer.clone())?;
        info!("Dialed {peer}");
    }

    // listen on all interfaces
    swarm.listen_on("/ip4/0.0.0.0/tcp/3210".parse()?)?;

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Listening on {address:?}");
            }
            SwarmEvent::Behaviour(FleygBehaviorEvent::KeepAlive) => {
                info!("KeepAlive");
            }
            SwarmEvent::Behaviour(FleygBehaviorEvent::Ping(event)) => {
                info!("Ping: {event:?}");
            }
            SwarmEvent::Behaviour(FleygBehaviorEvent::Identify(event)) => match event {
                identify::Event::Sent { peer_id, .. } => {
                    info!("Identify Sent: {peer_id}");
                }
                identify::Event::Received { info, .. } => {
                    info!("Identify Received: {info:?}");
                }
                _ => {}
            },
            _ => {}
        }
    }
}
