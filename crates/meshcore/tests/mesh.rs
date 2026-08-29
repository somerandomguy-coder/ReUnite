//! Behavioural tests for the pieces that are hard to eyeball on a live mesh.

use std::collections::HashSet;

use meshcore::beacon;
use meshcore::crypto;
use meshcore::geo::haversine_m;
use meshcore::identity::Identity;
use meshcore::net::NetworkBook;
use meshcore::packet::{Body, Frame, Hello, NetPayload, Packet, DEFAULT_TTL};
use meshcore::router::Router;
use meshcore::status;
use meshcore::types::{now_ms, Gps, MsgId, NodeId};
use meshcore::zones::{self, ZoneBook};

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
        battery: Some(73),
        sos: false,
        status: None,
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


// ---------------------------------------------------------------- phase 1 additions

#[test]
fn beacons_fit_a_ble_advertisement_and_round_trip_byte_exactly() {
    let presence = beacon::Beacon {
        header: beacon::Header {
            flags: beacon::FLAG_SOS | beacon::FLAG_GPS | beacon::FLAG_STATUS,
            battery: 42,
            seq: 200,
        },
        body: beacon::Body::Presence(beacon::Presence {
            node: NodeId::from_uuid("alice"),
            lat_e7: beacon::to_e7(10.7769),
            lon_e7: beacon::to_e7(106.7009),
            status: status::MEDICAL,
            hops: 2,
            ttl: 6,
        }),
    };
    let encoded = presence.encode();
    assert_eq!(encoded.len, beacon::PRESENCE_BYTES);
    assert!(
        encoded.len <= beacon::MAX_BEACON_BYTES,
        "a legacy BLE advert leaves {} usable bytes, this needs {}",
        beacon::MAX_BEACON_BYTES,
        encoded.len
    );
    assert_eq!(beacon::Beacon::decode(encoded.as_slice()).unwrap(), presence);

    let zone = beacon::Beacon {
        header: beacon::Header {
            flags: beacon::FLAG_RELAY,
            battery: beacon::BATTERY_UNKNOWN,
            seq: 1,
        },
        body: beacon::Body::Zone(beacon::Zone {
            origin: NodeId::from_uuid("bob"),
            cell: 0x8844c0a339fffff,
            level: 191,
            consensus: 7,
        }),
    };
    let encoded = zone.encode();
    assert_eq!(encoded.len, beacon::ZONE_BYTES);
    assert!(encoded.len <= beacon::MAX_BEACON_BYTES);
    assert_eq!(beacon::Beacon::decode(encoded.as_slice()).unwrap(), zone);

    // Truncation must be an error, never a half-read struct.
    assert_eq!(
        beacon::Beacon::decode(&encoded.as_slice()[..10]),
        Err(beacon::BeaconError::TooShort)
    );
    let mut wrong_version = encoded.bytes;
    wrong_version[0] = (9 << 4) | beacon::TYPE_ZONE;
    assert_eq!(
        beacon::Beacon::decode(&wrong_version[..beacon::ZONE_BYTES]),
        Err(beacon::BeaconError::UnsupportedVersion(9))
    );
}

#[test]
fn gps_survives_the_beacon_fixed_point_encoding() {
    for (lat, lon) in [(10.7769, 106.7009), (-33.8688, 151.2093), (0.0, -179.9999999)] {
        let round = (
            beacon::from_e7(beacon::to_e7(lat)),
            beacon::from_e7(beacon::to_e7(lon)),
        );
        // 1e-7 degrees is ~1cm; anything inside a metre is far better than GPS itself.
        assert!((round.0 - lat).abs() < 1e-6, "lat {lat} -> {}", round.0);
        assert!((round.1 - lon).abs() < 1e-6, "lon {lon} -> {}", round.1);
    }
}

#[test]
fn pre_canned_status_is_one_byte_and_parses_both_ways() {
    assert_eq!(status::parse("medical"), Some(status::MEDICAL));
    assert_eq!(status::parse("2"), Some(status::MEDICAL));
    assert_eq!(status::parse("MEDICAL"), Some(status::MEDICAL));
    assert_eq!(status::parse("none"), Some(status::NONE));
    assert_eq!(status::parse("nonsense"), None);
    assert_eq!(status::parse("200"), None, "codes outside the table are rejected");
    assert_eq!(status::describe(status::TRAPPED), "Trapped - need rescue");

    // plan.md §3.2: the wire carries the code, never the words.
    let payload = NetPayload::Status {
        code: status::MEDICAL,
    };
    let bytes = bincode::serialize(&payload).unwrap();
    let text = status::describe(status::MEDICAL);
    assert!(
        !bytes.windows(text.len()).any(|w| w == text.as_bytes()),
        "the human text must not appear on the wire"
    );
    assert!(
        bytes.len() <= 8,
        "a pre-canned message should be a handful of bytes, got {}",
        bytes.len()
    );
}

#[test]
fn zone_consensus_counts_people_not_reports() {
    let home = temp_home("zones");
    let mut book = ZoneBook::load(&home, zones::DEFAULT_RESOLUTION).unwrap();
    let cell = zones::cell_for(10.7769, 106.7009, zones::DEFAULT_RESOLUTION).unwrap();
    let (alice, bob, carol) = (
        NodeId::from_uuid("alice"),
        NodeId::from_uuid("bob"),
        NodeId::from_uuid("carol"),
    );

    book.record(cell, alice, zones::level_to_byte(4), 1_000);
    book.record(cell, bob, zones::level_to_byte(4), 1_100);
    book.record(cell, carol, zones::level_to_byte(2), 1_200);
    let zone = book.get(cell).unwrap();
    assert_eq!(zone.consensus(), 3, "three distinct nodes verified this cell");
    let mean = zones::byte_to_level(zone.level());
    assert!((mean - 3.33).abs() < 0.1, "mean of 4,4,2 is ~3.33, got {mean}");

    // A node shouting the same cell again replaces its own opinion and never inflates
    // the consensus - that is what makes the number worth showing.
    book.record(cell, carol, zones::level_to_byte(0), 1_300);
    let zone = book.get(cell).unwrap();
    assert_eq!(zone.consensus(), 3, "re-reporting must not manufacture agreement");
    assert!(zones::byte_to_level(zone.level()) < mean, "the lower report pulled it down");

    // Reports age out; a cell with nothing current left disappears entirely.
    assert!(book.prune(1_300 + zones::ZONE_TTL_MS + 1) >= 3);
    assert!(book.get(cell).is_none());
}

#[test]
fn zones_and_their_consensus_survive_a_restart() {
    let home = temp_home("zonepersist");
    let cell = zones::cell_for(-33.8688, 151.2093, zones::DEFAULT_RESOLUTION).unwrap();
    {
        let mut book = ZoneBook::load(&home, zones::DEFAULT_RESOLUTION).unwrap();
        book.record(cell, NodeId::from_uuid("a"), zones::level_to_byte(3), now_ms());
        book.record(cell, NodeId::from_uuid("b"), zones::level_to_byte(3), now_ms());
        book.save().unwrap();
    }
    let book = ZoneBook::load(&home, zones::DEFAULT_RESOLUTION).unwrap();
    let zone = book.get(cell).expect("zone survived the restart");
    assert_eq!(zone.consensus(), 2);
    assert_eq!(zone.level(), zones::level_to_byte(3));
}

#[test]
fn a_gps_fix_snaps_to_a_stable_hex_cell() {
    let res = zones::DEFAULT_RESOLUTION;
    let a = zones::cell_for(10.7769, 106.7009, res).unwrap();
    // A few metres away is the same cell: that is the whole point of aggregating.
    let b = zones::cell_for(10.77695, 106.70095, res).unwrap();
    assert_eq!(a, b, "neighbouring fixes must aggregate into one cell");
    // A few kilometres away is not.
    let far = zones::cell_for(10.8269, 106.7509, res).unwrap();
    assert_ne!(a, far);

    let (lat, lon) = zones::cell_center(a).unwrap();
    assert!(
        haversine_m(
            &Gps { lat, lon, ts_ms: 0 },
            &Gps { lat: 10.7769, lon: 106.7009, ts_ms: 0 }
        ) < 1_000.0,
        "the cell centre should be within a cell's radius of the report"
    );
}

#[test]
fn emergency_payloads_are_unreadable_outside_the_network() {
    // plan.md §3.2: a relay carries SOS, status and zone traffic for a private network
    // without being able to read any of it.
    let home = temp_home("emergency-privacy");
    let me = NodeId::from_uuid("me");
    let mut book = NetworkBook::load(&home, me).unwrap();
    let id = book.create("rescue", me).unwrap();
    let key = book.get(&id).unwrap().key;
    let outsider = crypto::random_key();

    for payload in [
        NetPayload::Sos {
            active: true,
            gps: Some(Gps { lat: 10.7769, lon: 106.7009, ts_ms: 1 }),
        },
        NetPayload::Status { code: status::TRAPPED },
        NetPayload::Zone { cell: 0x8844c0a339fffff, level: 200 },
    ] {
        let plaintext = bincode::serialize(&payload).unwrap();
        let (nonce, ciphertext) = crypto::sym_encrypt(&key, &plaintext).unwrap();
        assert!(
            crypto::sym_decrypt(&outsider, &nonce, &ciphertext).is_err(),
            "a non-member decrypted {payload:?}"
        );
        assert_eq!(
            crypto::sym_decrypt(&key, &nonce, &ciphertext).unwrap(),
            plaintext,
            "a member must still be able to read it"
        );
    }
}

#[test]
fn a_hello_carries_battery_sos_and_status() {
    let home = temp_home("hello-v3");
    let identity = Identity::load_or_create(&home).unwrap();
    let hello = Hello {
        ed_pub: identity.ed_public(),
        x_pub: identity.x_public(),
        name: Some("alice".into()),
        gps: Some(Gps { lat: 10.7769, lon: 106.7009, ts_ms: 5 }),
        battery: Some(4),
        sos: true,
        status: Some(status::TRAPPED),
    };
    let packet = signed_packet(&identity, None, Body::Hello(hello.clone()));
    let bytes = Frame::new(identity.id, 1, packet).encode().unwrap();
    let decoded = Frame::decode(&bytes).unwrap();
    match decoded.packet.body {
        Body::Hello(h) => {
            assert_eq!(h.battery, Some(4));
            assert!(h.sos);
            assert_eq!(h.status, Some(status::TRAPPED));
        }
        other => panic!("unexpected body {other:?}"),
    }
    // The state fields are inside the signature, so a relay cannot clear someone's SOS.
    let mut forged = Frame::decode(&bytes).unwrap().packet;
    forged.body = Body::Hello(Hello { sos: false, ..hello.clone() });
    assert!(crypto::verify(&hello.ed_pub, &forged.signing_bytes(), &forged.sig).is_err());
}
