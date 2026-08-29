//! Two complete nodes meshing over a transport that has no networking in it at all.
//!
//! This is the check that the radio seam really is a seam. The frames here are pumped
//! between two `ExternalTransport`s by hand, which is precisely what the Kotlin and Swift
//! BLE layers do once a phone is holding the other end. If discovery, chat and SOS work
//! here, then everything above the radio - routing, dedupe, signatures, encryption, the
//! node actor - is genuinely transport-agnostic, and only the platform I/O is left to
//! verify on real hardware.

use std::sync::Arc;
use std::time::Duration;

use meshcore::node::{Command, Event, Node, NodeConfig};
use meshcore::transport::{ExternalTransport, Transport};
use meshcore::types::now_ms;

fn temp_home(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "meshcore-ext-{tag}-{}-{}",
        std::process::id(),
        now_ms()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Shuttle frames between the two transports, the way a BLE layer would.
fn pump(a: Arc<ExternalTransport>, b: Arc<ExternalTransport>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let mut moved = false;
            while let Some(out) = a.take_outbound() {
                let _ = b.inject(out.frame, "peer-a");
                moved = true;
            }
            while let Some(out) = b.take_outbound() {
                let _ = a.inject(out.frame, "peer-b");
                moved = true;
            }
            if !moved {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    })
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_nodes_mesh_over_a_transport_with_no_networking() {
    let ext_a = Arc::new(ExternalTransport::new("ble/test-a"));
    let ext_b = Arc::new(ExternalTransport::new("ble/test-b"));

    let mut cfg_a = NodeConfig::new(temp_home("a"));
    cfg_a.self_name = Some("alice".into());
    cfg_a.hello_interval = Duration::from_millis(300);
    cfg_a.battery_override = Some(90);

    let mut cfg_b = NodeConfig::new(temp_home("b"));
    cfg_b.self_name = Some("bob".into());
    cfg_b.hello_interval = Duration::from_millis(300);
    cfg_b.battery_override = Some(11);

    let (a, mut events_a) = Node::spawn(cfg_a, ext_a.clone() as Arc<dyn Transport>).unwrap();
    let (b, mut events_b) = Node::spawn(cfg_b, ext_b.clone() as Arc<dyn Transport>).unwrap();
    let pumper = pump(ext_a.clone(), ext_b.clone());

    // 1. Discovery: each learns the other purely from beacons crossing the seam.
    let joined = wait_for(&mut events_a, |e| matches!(e, Event::PeerJoined { .. })).await;
    match joined {
        Event::PeerJoined { display, .. } => assert_eq!(display, "~bob"),
        _ => unreachable!(),
    }
    wait_for(&mut events_b, |e| matches!(e, Event::PeerJoined { .. })).await;

    // 2. Chat, signed and encrypted, reaches the far side intact.
    a.call(Command::Broadcast("water at block 4".into())).await.unwrap();
    let chat = wait_for(&mut events_b, |e| matches!(e, Event::Chat { .. })).await;
    match chat {
        Event::Chat { from, text, .. } => {
            assert_eq!(from, "~alice");
            assert_eq!(text, "water at block 4");
        }
        _ => unreachable!(),
    }

    // 3. SOS, the packet class that matters most, crosses too.
    b.call(Command::Sos(true)).await.unwrap();
    let sos = wait_for(&mut events_a, |e| matches!(e, Event::SosRaised { .. })).await;
    match sos {
        Event::SosRaised { display, .. } => assert_eq!(display, "~bob"),
        _ => unreachable!(),
    }

    // 4. Battery telemetry rode the beacon, and it is bob's low battery A can see.
    let peers = match a.call(Command::Peers).await.unwrap() {
        meshcore::node::Reply::Peers(p) => p,
        other => panic!("unexpected reply {other:?}"),
    };
    let bob = peers.iter().find(|p| p.display == "~bob").expect("bob is a peer");
    assert_eq!(bob.battery, Some(11));
    assert!(bob.sos, "bob's SOS flag is visible on the peer list");
    assert!(!bob.ghost, "bob is reachable, so not a ghost");

    pumper.abort();
}

#[tokio::test]
async fn the_outbound_queue_is_bounded_when_the_radio_stops_draining() {
    // Bluetooth off, or permission refused: the platform never drains. The queue must
    // not grow without limit, and the node must keep running.
    let ext = ExternalTransport::new("ble/stalled");
    for i in 0..1000 {
        ext.send_broadcast(format!("frame {i}").as_bytes()).await.unwrap();
    }
    assert!(ext.pending() <= 256, "queue grew to {}", ext.pending());
    // The newest frames survive; the stale ones are the ones dropped.
    let first = ext.take_outbound().unwrap();
    assert!(
        String::from_utf8_lossy(&first.frame).starts_with("frame 7"),
        "expected the oldest surviving frame, got {:?}",
        String::from_utf8_lossy(&first.frame)
    );
}

#[tokio::test]
async fn a_device_that_disconnects_loses_its_link_address() {
    let ext = ExternalTransport::new("ble/churn");
    ext.inject(b"hello".to_vec(), "AA:BB:CC:DD:EE:FF").unwrap();
    let (_, addr) = ext.recv().await.unwrap();

    // Sending to a known device names it, so the platform can target one connection.
    ext.send_to(b"reply", addr).await.unwrap();
    assert_eq!(ext.take_outbound().unwrap().to.as_deref(), Some("AA:BB:CC:DD:EE:FF"));

    // After a disconnect the address is stale, and a frame for it degrades to a
    // broadcast rather than vanishing.
    ext.peer_lost("AA:BB:CC:DD:EE:FF");
    ext.send_to(b"reply", addr).await.unwrap();
    assert_eq!(ext.take_outbound().unwrap().to, None);
}
