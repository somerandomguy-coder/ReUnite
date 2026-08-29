//! Behavioural tests for the pieces that are hard to eyeball on a live mesh.

use std::collections::HashSet;

use meshcore::crypto;
use meshcore::geo::haversine_m;
use meshcore::identity::Identity;
use meshcore::net::NetworkBook;
use meshcore::packet::{Body, Frame, Hello, NetPayload, Packet, DEFAULT_TTL};
use meshcore::router::Router;
use meshcore::types::{now_ms, Gps, MsgId, NodeId};

fn temp_home(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "meshcore-test-{tag}-{}-{}",
        std::process::id(),
        now_ms()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn signed_packet(identity: &Identity, dest: Option<NodeId>, body: Body) -> Packet {
    let mut packet = Packet {
        id: MsgId::random(),
        origin: identity.id,
        dest,
        sent_at_ms: now_ms(),
        body,
        sig: Vec::new(),
        ttl: DEFAULT_TTL,
        path: Vec::new(),
    };
    packet.sig = identity.sign(&packet.signing_bytes());
    packet
}

#[test]
fn sealed_box_only_opens_for_its_recipient() {
    let recipient = crypto::new_exchange_secret();
    let eavesdropper = crypto::new_exchange_secret();
    let sealed = crypto::seal_to(&crypto::exchange_public(&recipient), b"network key").unwrap();

    assert_eq!(
        crypto::open_sealed(&recipient, &sealed).unwrap(),
        b"network key"
    );
    assert!(crypto::open_sealed(&eavesdropper, &sealed).is_err());
}

#[test]
fn network_traffic_needs_the_network_key() {
    let key = crypto::random_key();
    let other = crypto::random_key();
    let (nonce, ct) = crypto::sym_encrypt(&key, b"meet at the school").unwrap();

    assert_eq!(crypto::sym_decrypt(&key, &nonce, &ct).unwrap(), b"meet at the school");
    assert!(crypto::sym_decrypt(&other, &nonce, &ct).is_err());
}

#[test]
fn packets_survive_a_round_trip_and_reject_tampering() {
    let home = temp_home("packet");
    let identity = Identity::load_or_create(&home).unwrap();
    let hello = Hello {
        ed_pub: identity.ed_public(),
        x_pub: identity.x_public(),
        name: Some("alice".into()),
        gps: None,
    };
    let packet = signed_packet(&identity, None, Body::Hello(hello.clone()));

    let bytes = Frame::new(identity.id, 42, packet.clone()).encode().unwrap();
    let decoded = Frame::decode(&bytes).unwrap();
    assert_eq!(decoded.link_from, identity.id);
    crypto::verify(&hello.ed_pub, &decoded.packet.signing_bytes(), &decoded.packet.sig).unwrap();

    // A relay may rewrite ttl/path (outside the signature) but not the payload.
    let mut relayed = decoded.packet.clone();
    relayed.ttl -= 1;
    relayed.path.push(NodeId::from_uuid("relay"));
    crypto::verify(&hello.ed_pub, &relayed.signing_bytes(), &relayed.sig).unwrap();

    let mut forged = decoded.packet;
    forged.body = Body::Ping { nonce: 7 };
    assert!(crypto::verify(&hello.ed_pub, &forged.signing_bytes(), &forged.sig).is_err());
}

#[test]
fn router_suppresses_duplicate_floods_and_prefers_shorter_paths() {
    let me = NodeId::from_uuid("me");
    let near = NodeId::from_uuid("near");
    let far = NodeId::from_uuid("far");
    let mut router = Router::new(me);

    let id = MsgId::random();
    assert!(router.mark_seen(id), "first copy is delivered");
    assert!(!router.mark_seen(id), "the flood's second copy is dropped");

    router.note_neighbor(near, "127.0.0.1:1".parse().unwrap());
    router.learn_route(far, near, 3);
    assert_eq!(router.route(&far).unwrap().hops, 3);
    router.learn_route(far, near, 2);
    assert_eq!(router.route(&far).unwrap().hops, 2, "shorter route wins");
    assert!(router.has_route(&far), "routed through a known neighbour");
}

#[test]
fn link_filter_simulates_radio_range() {
    let me = NodeId::from_uuid("me");
    let audible = NodeId::from_uuid("audible");
    let distant = NodeId::from_uuid("distant");
    let mut router = Router::new(me);

    assert!(router.accepts_link(&distant), "no filter means everyone is heard");
    router.set_link_filter(HashSet::from([audible]));
    assert!(router.accepts_link(&audible));
    assert!(!router.accepts_link(&distant));
}

#[test]
fn kick_needs_half_the_members_and_re_keys_the_network() {
    let home = temp_home("kick");
    let alice = NodeId::from_uuid("alice");
    let bob = NodeId::from_uuid("bob");
    let carol = NodeId::from_uuid("carol");
    let mut book = NetworkBook::load(&home, alice).unwrap();
    let id = book.create("rescue", alice).unwrap();
    {
        let net = book.get_mut(&id).unwrap();
        net.members.insert(bob);
        net.members.insert(carol);
    }

    let before = book.get(&id).unwrap().key;
    assert_eq!(book.get(&id).unwrap().kick_threshold(), 2, "3 members -> 2 votes");
    // Lowest remaining id mints the new key, and everyone computes the same answer.
    let leader = book.get(&id).unwrap().rekey_leader(&bob).unwrap();
    assert_eq!(leader, [alice, carol].into_iter().min().unwrap());

    let epoch = book.rekey(&id, &bob).unwrap();
    let net = book.get(&id).unwrap();
    assert_eq!(epoch, 1);
    assert!(!net.members.contains(&bob), "kicked member is dropped");
    assert_ne!(net.key, before, "a fresh key locks the kicked node out");
    assert_eq!(
        net.key_for_epoch(0),
        Some(before),
        "old generation is kept so in-flight packets still open"
    );
}

#[test]
fn networks_and_their_keys_survive_a_restart() {
    let home = temp_home("persist");
    let me = NodeId::from_uuid("me");
    let (id, key) = {
        let mut book = NetworkBook::load(&home, me).unwrap();
        let id = book.create("rescue", me).unwrap();
        book.get_mut(&id).unwrap().store_messages = true;
        book.save().unwrap();
        (id, book.get(&id).unwrap().key)
    };

    let reopened = NetworkBook::load(&home, me).unwrap();
    let net = reopened.get(&id).expect("network reloaded from disk");
    assert_eq!(net.name, "rescue");
    assert_eq!(net.key, key);
    assert!(net.store_messages);
    assert!(reopened.by_name("default").is_some(), "[default] always exists");
}

#[test]
fn identity_is_stable_across_restarts() {
    let home = temp_home("identity");
    let first = Identity::load_or_create(&home).unwrap();
    let second = Identity::load_or_create(&home).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(first.ed_public(), second.ed_public());
    assert_eq!(first.id, NodeId::from_uuid(&first.uuid), "id is the hashed UUID");
}

#[test]
fn payloads_round_trip_through_bincode() {
    let payload = NetPayload::Direct {
        text: "bring water to block 4".into(),
    };
    let bytes = bincode::serialize(&payload).unwrap();
    match bincode::deserialize::<NetPayload>(&bytes).unwrap() {
        NetPayload::Direct { text } => assert_eq!(text, "bring water to block 4"),
        other => panic!("unexpected payload {other:?}"),
    }
}

#[test]
fn distance_between_gps_fixes_is_sane() {
    let a = Gps {
        lat: 10.7769,
        lon: 106.7009,
        ts_ms: 0,
    };
    let b = Gps {
        lat: 10.7869,
        lon: 106.7009,
        ts_ms: 0,
    };
    let d = haversine_m(&a, &b);
    assert!((1050.0..1160.0).contains(&d), "0.01 degree of latitude is ~1.1km, got {d}");
}
