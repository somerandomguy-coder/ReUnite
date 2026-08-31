//! The iOS Wi-Fi path: unicast to an explicit seed, with multicast and broadcast off.
//!
//! Since iOS 16 Apple gates both multicast *and* limited broadcast - sending as well as
//! receiving - behind the restricted `com.apple.developer.networking.multicast`
//! entitlement, which this app does not have. So the only UDP an iPhone may legally emit
//! is plain unicast to an address it was told about: `StartConfig.peers`, which becomes
//! `UdpConfig::seeds`. That makes `multicast: false, broadcast: false, seeds: [addr]` the
//! production configuration on every iPhone, and the tests below are what stand behind
//! it.
//!
//! The asymmetry is the point. Only the phone gets a seed; the MacBook is started with
//! nothing and cannot address anyone at all until a frame arrives. Its reply path exists
//! solely because `UdpTransport::recv` remembers the source of everything it hears, so if
//! that memory ever regressed the phone would appear to mesh - its own beacons would land
//! - while nothing ever came back.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use meshcore::node::{Command, Event, Node, NodeConfig};
use meshcore::transport::{Transport, UdpConfig, UdpTransport};
use meshcore::types::now_ms;
use tokio::time::{timeout, Instant};

fn temp_home(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "meshcore-udp-{tag}-{}-{}",
        std::process::id(),
        now_ms()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The radio exactly as `meshffi` builds it for an iPhone: no group membership, no
/// limited broadcast, and only the addresses the user (or a QR code) supplied.
///
/// `port: 0` is not incidental. A developer usually has a real node holding 47474, and a
/// test that grabbed the default port would either fail to bind or - far worse - quietly
/// mesh with the very thing it is meant to be simulating.
fn ios_radio(seeds: Vec<SocketAddr>) -> UdpConfig {
    UdpConfig {
        port: 0,
        multicast: false,
        broadcast: false,
        seeds,
        ..UdpConfig::default()
    }
}

/// Turn a bound transport's local address into something a peer can actually be seeded
/// with. The socket is bound to `0.0.0.0`, so `local_addr()` reports the wildcard, and a
/// datagram sent to `0.0.0.0` goes nowhere useful. Only the port is ours to keep.
fn seed_for(transport: &UdpTransport) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, transport.local_addr().port()))
}

/// Broadcast until the frame lands, and return the address it appeared to come from.
///
/// UDP is allowed to drop a datagram even on loopback - one full socket buffer is enough -
/// and the mesh's own answer to that is to keep beaconing. The test uses the same answer
/// rather than a sleep long enough to hope.
async fn deliver(from: &UdpTransport, to: &UdpTransport, frame: &[u8]) -> SocketAddr {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "frame never arrived: {}",
            String::from_utf8_lossy(frame)
        );
        from.send_broadcast(frame).await.unwrap();
        if let Ok(Ok((bytes, src))) = timeout(Duration::from_millis(100), to.recv()).await {
            assert_eq!(bytes, frame, "a different frame arrived");
            return src;
        }
    }
}

async fn wait_for<F>(events: &mut tokio::sync::mpsc::Receiver<Event>, mut matches: F) -> Event
where
    F: FnMut(&Event) -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(!remaining.is_zero(), "timed out waiting for an event");
        match tokio::time::timeout(remaining, events.recv()).await {
            Ok(Some(event)) => {
                if matches(&event) {
                    return event;
                }
            }
            Ok(None) => panic!("event stream closed"),
            Err(_) => panic!("timed out waiting for an event"),
        }
    }
}

/// Phase 2: the raw transport half of the iOS path, with no node machinery in the way.
///
/// Forward is the easy direction - the seed is a target. The reverse is the one that has
/// never been exercised: the Mac has no multicast, no broadcast and no seeds, so the only
/// address `send_broadcast` can possibly hand the socket is one that `recv` learned.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_seeded_phone_gets_answered_by_a_mac_that_was_given_nobody_s_address() {
    // The MacBook: entitlement-free configuration and an empty address book. On its own
    // it cannot start a conversation; it can only ever answer.
    let mac = UdpTransport::bind(ios_radio(Vec::new())).unwrap();
    // The iPhone: the same configuration, plus the one address it was handed.
    let phone = UdpTransport::bind(ios_radio(vec![seed_for(&mac)])).unwrap();

    let heard = deliver(&phone, &mac, b"hello from the phone").await;
    assert_eq!(
        heard.port(),
        phone.local_addr().port(),
        "the Mac must see the phone's own source port, since that is the only return \
         address it will ever get",
    );
    assert!(heard.ip().is_loopback(), "unexpected source {heard}");

    // The reverse path, which the whole iOS story depends on.
    let back = deliver(&mac, &phone, b"ack from the mac").await;
    assert_eq!(
        back.port(),
        mac.local_addr().port(),
        "the reply must come from the Mac itself, not from some relayed address",
    );
}

/// Phase 2: the same asymmetry driven through two real nodes.
///
/// A `Hello` has to cross, be verified, and turn into a contact on both sides - the
/// transport being able to move bytes is necessary but not sufficient. The Mac's beacons
/// go nowhere at all until the phone's first one arrives, which is precisely the join race
/// a phone-and-laptop pair runs in the field.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_nodes_mesh_over_unicast_seeds_with_multicast_and_broadcast_off() {
    let mac_radio = UdpTransport::bind(ios_radio(Vec::new())).unwrap();
    let phone_radio = UdpTransport::bind(ios_radio(vec![seed_for(&mac_radio)])).unwrap();

    let mut cfg_mac = NodeConfig::new(temp_home("mac"));
    cfg_mac.self_name = Some("mac".into());
    cfg_mac.hello_interval = Duration::from_millis(300);
    cfg_mac.battery_override = Some(88);

    let mut cfg_phone = NodeConfig::new(temp_home("phone"));
    cfg_phone.self_name = Some("phone".into());
    cfg_phone.hello_interval = Duration::from_millis(300);
    cfg_phone.battery_override = Some(23);

    let (mac, mut events_mac) = Node::spawn(cfg_mac, Arc::new(mac_radio) as Arc<dyn Transport>).unwrap();
    let (phone, mut events_phone) =
        Node::spawn(cfg_phone, Arc::new(phone_radio) as Arc<dyn Transport>).unwrap();

    // 1. The Mac hears the phone. This direction only needs the seed to work.
    let joined = wait_for(&mut events_mac, |e| matches!(e, Event::PeerJoined { .. })).await;
    match joined {
        Event::PeerJoined { display, .. } => assert_eq!(display, "~phone"),
        _ => unreachable!(),
    }

    // 2. The phone hears the Mac. Nothing configured on the Mac can produce this frame;
    //    it exists only because the link address was learned on the way in.
    let joined = wait_for(&mut events_phone, |e| matches!(e, Event::PeerJoined { .. })).await;
    match joined {
        Event::PeerJoined { display, .. } => assert_eq!(display, "~mac"),
        _ => unreachable!(),
    }

    // 3. Chat crosses the learned link too, not just the beacon that created it - a
    //    reply that only ever worked for `Hello` would still be a broken mesh.
    mac.call(Command::Broadcast("shelter is open".into()))
        .await
        .unwrap();
    let chat = wait_for(&mut events_phone, |e| matches!(e, Event::Chat { .. })).await;
    match chat {
        Event::Chat { from, text, .. } => {
            assert_eq!(from, "~mac");
            assert_eq!(text, "shelter is open");
        }
        _ => unreachable!(),
    }

    // 4. And back up the seeded direction, so the pair is genuinely bidirectional at the
    //    application layer and not merely at the socket.
    phone
        .call(Command::Broadcast("on my way".into()))
        .await
        .unwrap();
    let chat = wait_for(&mut events_mac, |e| matches!(e, Event::Chat { .. })).await;
    match chat {
        Event::Chat { from, text, .. } => {
            assert_eq!(from, "~phone");
            assert_eq!(text, "on my way");
        }
        _ => unreachable!(),
    }

    // 5. Each side lists the other, which is what the app's peer list actually renders.
    for (who, handle, expected, battery) in [
        ("mac", &mac, "~phone", 23u8),
        ("phone", &phone, "~mac", 88u8),
    ] {
        let peers = match handle.call(Command::Peers).await.unwrap() {
            meshcore::node::Reply::Peers(p) => p,
            other => panic!("unexpected reply {other:?}"),
        };
        let peer = peers
            .iter()
            .find(|p| p.display == expected)
            .unwrap_or_else(|| panic!("{who} does not list {expected}: {peers:?}"));
        assert_eq!(peer.battery, Some(battery), "telemetry rode the beacon");
        assert!(!peer.ghost, "{expected} answered, so it is not a ghost");
    }
}

/// A radio with multicast off, broadcast off and no seeds has no way to reach anyone, and
/// that is exactly the state an un-entitled iPhone starts in when nobody supplied a peer.
/// `send_broadcast` swallows every error, so nothing else in the system will ever notice.
/// The banner is the only place a user can find out, so it has to say so.
#[tokio::test]
async fn a_radio_with_no_multicast_no_broadcast_and_no_seeds_admits_it_cannot_find_anyone() {
    let stranded = UdpTransport::bind(ios_radio(Vec::new())).unwrap();
    assert!(
        stranded.describe().contains("no discovery path"),
        "a radio that can reach nobody must not look identical to a healthy one: {}",
        stranded.describe(),
    );

    // One seed is enough to make it a working radio, and it must not be labelled broken.
    let seeded = UdpTransport::bind(ios_radio(vec![seed_for(&stranded)])).unwrap();
    let described = seeded.describe();
    assert!(described.contains("seeds"), "{described}");
    assert!(!described.contains("no discovery path"), "{described}");

    // Nor may the label outlive the problem: once a peer has found us, the link it left
    // behind is a real, usable path, even though the configuration never changed.
    deliver(&seeded, &stranded, b"found you").await;
    assert!(
        !stranded.describe().contains("no discovery path"),
        "a learned link is a discovery path: {}",
        stranded.describe(),
    );
}
