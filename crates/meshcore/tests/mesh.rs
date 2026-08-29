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
            verdict: zones::WIRE_UNSAFE,
            consensus: 7,
            radius_m: 750,
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
    assert_eq!(status::parse("sos"), Some(status::MEDICAL));
    assert_eq!(status::parse("2"), Some(status::MEDICAL));
    assert_eq!(status::parse("SOS"), Some(status::MEDICAL));
    assert_eq!(status::parse("none"), Some(status::NONE));
    assert_eq!(status::parse("nonsense"), None);
    assert_eq!(status::parse("200"), None, "codes outside the table are rejected");
    assert_eq!(status::describe(status::SAFE), "🟢 Safe & Moving");

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
fn a_zone_counts_people_on_each_side_and_never_blends_them() {
    let home = temp_home("zones");
    let mut book = ZoneBook::load(&home, zones::DEFAULT_RESOLUTION).unwrap();
    let cell = zones::cell_for(10.7769, 106.7009, zones::DEFAULT_RESOLUTION).unwrap();
    let (alice, bob, carol) = (
        NodeId::from_uuid("alice"),
        NodeId::from_uuid("bob"),
        NodeId::from_uuid("carol"),
    );

    book.record(cell, alice, zones::Verdict::Safe, 500, 1_000);
    book.record(cell, bob, zones::Verdict::Safe, 300, 1_100);
    book.record(cell, carol, zones::Verdict::Unsafe, 400, 1_200);
    let zone = book.get(cell).unwrap();
    assert_eq!(zone.safe_votes(), 2);
    assert_eq!(zone.unsafe_votes(), 1);
    assert_eq!(zone.verdict(), zones::Verdict::Safe, "2 safe beats 1 unsafe");
    assert_eq!(zone.consensus(), 3, "three distinct nodes have an opinion");
    // The radius is the mean of the reports that *agree* with the verdict: 500 and 300,
    // not carol's 400, which was describing a different claim about the same ground.
    assert_eq!(zone.radius_m(), 400);

    // A node changing its mind replaces its own vote and never inflates either count.
    book.record(cell, carol, zones::Verdict::Safe, 400, 1_300);
    let zone = book.get(cell).unwrap();
    assert_eq!(zone.safe_votes(), 3);
    assert_eq!(zone.unsafe_votes(), 0);
    assert_eq!(zone.consensus(), 3, "re-reporting must not manufacture agreement");

    // Reports age out; a cell with nothing current left disappears entirely.
    assert!(book.prune(1_300 + zones::ZONE_TTL_MS + 1) >= 3);
    assert!(book.get(cell).is_none());
}

#[test]
fn a_contested_cell_reads_unsafe_rather_than_splitting_the_difference() {
    let home = temp_home("zonetie");
    let mut book = ZoneBook::load(&home, zones::DEFAULT_RESOLUTION).unwrap();
    let cell = zones::cell_for(51.5074, -0.1278, zones::DEFAULT_RESOLUTION).unwrap();

    for (i, verdict) in [
        zones::Verdict::Safe,
        zones::Verdict::Safe,
        zones::Verdict::Unsafe,
        zones::Verdict::Unsafe,
    ]
    .into_iter()
    .enumerate()
    {
        book.record(cell, NodeId::from_uuid(&format!("n{i}")), verdict, 250, 1_000);
    }

    let zone = book.get(cell).unwrap();
    assert_eq!(zone.safe_votes(), 2);
    assert_eq!(zone.unsafe_votes(), 2);
    assert_eq!(
        zone.verdict(),
        zones::Verdict::Unsafe,
        "a tie must resolve to unsafe - painting a contested street green is what hurts someone",
    );

    // And the disagreement stays visible rather than collapsing into one number.
    let views = book.views(&NodeId::from_uuid("observer"), 1_000);
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].safe_votes, 2);
    assert_eq!(views[0].unsafe_votes, 2);
}

#[test]
fn a_node_re_gossips_only_its_sixteen_most_recent_reports() {
    let home = temp_home("zonering");
    let mut book = ZoneBook::load(&home, zones::DEFAULT_RESOLUTION).unwrap();
    let me = NodeId::from_uuid("me");

    // Twenty distinct cells, walked in order.
    let cells: Vec<u64> = (0..20)
        .map(|i| {
            zones::cell_for(10.0 + i as f64 * 0.05, 106.0, zones::DEFAULT_RESOLUTION).unwrap()
        })
        .collect();
    for (i, cell) in cells.iter().enumerate() {
        book.record_own(*cell, me, zones::Verdict::Safe, 200, 1_000 + i as u64);
    }

    let mine = book.mine(&me);
    assert_eq!(
        mine.len(),
        zones::OWN_REPORT_CAPACITY,
        "the re-gossip ring is bounded, or a node that has walked a city gossips forever",
    );
    let gossiped: Vec<u64> = mine.iter().map(|(c, _, _)| *c).collect();
    assert!(!gossiped.contains(&cells[0]), "the oldest fell out of the ring");
    assert!(gossiped.contains(&cells[19]), "the newest is in it");

    // Eviction stops republishing. It must NOT withdraw the report itself - other nodes
    // are still counting that vote, and silently retracting it would rewrite their map.
    assert!(
        book.get(cells[0]).unwrap().reports.contains_key(&me),
        "an evicted report still stands, it is just no longer rebroadcast",
    );

    // Re-reporting a cell already in the ring moves it to the newest end instead of
    // taking a second slot.
    let before = book.mine(&me).len();
    book.record_own(cells[19], me, zones::Verdict::Unsafe, 200, 2_000);
    assert_eq!(book.mine(&me).len(), before, "no duplicate slot for one cell");
    assert_eq!(book.mine(&me).last().unwrap().0, cells[19]);
}

#[test]
fn a_radius_survives_whatever_unit_it_was_typed_in() {
    // The same distance, three ways a person might type it.
    let m = zones::to_metres(500.0, "m").unwrap();
    let km = zones::to_metres(0.5, "km").unwrap();
    let ft = zones::to_metres(1640.42, "ft").unwrap();
    assert_eq!(m, 500);
    assert_eq!(km, 500);
    assert!((ft as i64 - 500).abs() <= 1, "1640.42 ft is 500 m, got {ft}");
    assert_eq!(zones::to_metres(1.0, "mi").unwrap(), 1609);

    // The limits are refusals, not silent clamps: someone who types the wrong unit must
    // be told, not quietly given a different area than they asked for.
    assert!(zones::to_metres(1.0, "m").is_err(), "below the minimum");
    assert!(zones::to_metres(50.0, "km").is_err(), "past the maximum");
    assert!(zones::to_metres(0.0, "m").is_err());
    assert!(zones::to_metres(-5.0, "m").is_err());
    assert!(zones::to_metres(100.0, "furlongs").is_err());

    // A radius arriving off the wire from another build is clamped rather than refused -
    // dropping the report would lose a hazard over a formatting disagreement.
    assert_eq!(zones::clamp_radius(0), zones::MIN_RADIUS_M);
    assert_eq!(zones::clamp_radius(u32::MAX), zones::MAX_RADIUS_M);
}

#[test]
fn zones_and_their_votes_survive_a_restart() {
    let home = temp_home("zonepersist");
    let cell = zones::cell_for(-33.8688, 151.2093, zones::DEFAULT_RESOLUTION).unwrap();
    let me = NodeId::from_uuid("a");
    {
        let mut book = ZoneBook::load(&home, zones::DEFAULT_RESOLUTION).unwrap();
        book.record_own(cell, me, zones::Verdict::Unsafe, 750, now_ms());
        book.record(cell, NodeId::from_uuid("b"), zones::Verdict::Unsafe, 250, now_ms());
        book.save().unwrap();
    }
    let book = ZoneBook::load(&home, zones::DEFAULT_RESOLUTION).unwrap();
    let zone = book.get(cell).expect("zone survived the restart");
    assert_eq!(zone.unsafe_votes(), 2);
    assert_eq!(zone.verdict(), zones::Verdict::Unsafe);
    assert_eq!(zone.radius_m(), 500, "mean of 750 and 250");
    // The ring is persisted too, or a restart would republish in a different order.
    assert_eq!(book.mine(&me), vec![(cell, zones::Verdict::Unsafe, 750)]);
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
        NetPayload::Zone {
            cell: 0x8844c0a339fffff,
            verdict: zones::WIRE_UNSAFE,
            radius_m: 800,
        },
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

// ---------------------------------------------------------------- phase 2D

#[test]
fn the_radio_eases_off_when_alone_and_snaps_back_when_anyone_appears() {
    use meshcore::duty::{self, Conditions, ScanMode};

    let alone = |ms: u64| {
        duty::cadence(Conditions {
            alone_for_ms: ms,
            peers: 0,
            sos: false,
            battery: None,
        })
    };

    // The first minute alone is the normal join race, not solitude: backing off into it
    // would make two phones started together slower to find each other, which is the one
    // moment they must not be.
    assert_eq!(alone(0), duty::Cadence::ENGAGED);
    assert_eq!(alone(59_000), duty::Cadence::ENGAGED);

    // Then it climbs down, and the scan eases with the beacon - duty-cycling only the
    // beacon saves very little, because listening is the expensive half.
    assert_eq!(alone(2 * 60_000).hello.as_secs(), 10);
    assert_eq!(alone(2 * 60_000).scan, ScanMode::Balanced);
    assert_eq!(alone(10 * 60_000).hello.as_secs(), 30);
    assert!(alone(10 * 60_000).scan_window.is_some());
    assert_eq!(alone(60 * 60_000).hello.as_secs(), 60);

    // It is monotonic: no rung is faster than the one before it.
    let mut previous = alone(0).hello;
    for minutes in [1, 3, 6, 15, 21, 120] {
        let now = alone(minutes * 60_000).hello;
        assert!(now >= previous, "cadence sped up at {minutes} minutes alone");
        previous = now;
    }

    // One peer, after an hour alone, and it is back to the top rung immediately.
    assert_eq!(
        duty::cadence(Conditions {
            alone_for_ms: 60 * 60_000,
            peers: 1,
            sos: false,
            battery: None,
        }),
        duty::Cadence::ENGAGED,
    );
}

#[test]
fn an_sos_never_backs_off_however_alone_it_is() {
    use meshcore::duty::{self, Conditions};

    // Ours, or a peer's we are relaying. An SOS is exactly the moment to spend the
    // battery, and a node that has been alone for hours is the one most likely to be
    // raising one.
    for battery in [None, Some(3), Some(100)] {
        assert_eq!(
            duty::cadence(Conditions {
                alone_for_ms: 24 * 60 * 60_000,
                peers: 0,
                sos: true,
                battery,
            }),
            duty::Cadence::ENGAGED,
            "an SOS backed off with battery {battery:?}",
        );
    }
}

#[test]
fn a_flat_battery_drops_one_further_rung_but_never_off_the_ladder() {
    use meshcore::duty::{self, Conditions};

    let at = |ms: u64, battery: Option<u8>| {
        duty::cadence(Conditions {
            alone_for_ms: ms,
            peers: 0,
            sos: false,
            battery,
        })
    };

    // This is what makes the battery byte in the beacon worth carrying: a nearly flat
    // node should still be findable in an hour, and it will not be if it spends what is
    // left talking to nobody.
    assert!(at(2 * 60_000, Some(5)).hello > at(2 * 60_000, Some(80)).hello);
    assert_eq!(at(2 * 60_000, Some(80)).hello, at(2 * 60_000, None).hello);

    // The bottom rung is a floor, not a cliff - a flat node still beacons.
    let bottom = at(24 * 60 * 60_000, Some(1));
    assert_eq!(bottom.hello.as_secs(), 60);
}

#[test]
fn jitter_spreads_nodes_without_drifting_the_rate() {
    use core::time::Duration;
    use meshcore::duty;

    let base = Duration::from_secs(10);
    let spread: Vec<u64> = (0..200)
        .map(|seed| duty::jitter(base, seed).as_millis() as u64)
        .collect();

    // Every value inside ±20 %...
    for value in &spread {
        assert!((8_000..=12_000).contains(value), "jittered to {value}ms");
    }
    // ...and genuinely spread, or twenty phones that started together would beacon in
    // lockstep and collide on air every single time - worst exactly when the room is
    // fullest.
    let distinct: std::collections::HashSet<u64> = spread.iter().copied().collect();
    assert!(distinct.len() > 50, "only {} distinct delays", distinct.len());

    // The mean stays put, so backing off does not secretly change the rate.
    let mean = spread.iter().sum::<u64>() as f64 / spread.len() as f64;
    assert!((mean - 10_000.0).abs() < 400.0, "mean drifted to {mean}ms");
}

/// A `zones.json` written by a build from before commit `df0bcbb` used `{"level": u8}`
/// per report; the current `Report` needs `verdict` and `radius_m`. Upgrading an install
/// across that change must not brick the node.
///
/// This is not hypothetical: it took an iPhone out entirely. `ZoneBook::load` returned
/// the serde error, `Node::spawn` passed it up, and the app showed "the mesh core did not
/// start" on every launch, forever, with no way back other than deleting the app.
#[test]
fn an_out_of_date_zones_file_does_not_stop_the_node_from_starting() {
    let home = temp_home("zones-legacy");
    std::fs::write(
        home.join("zones.json"),
        r#"{"zones":[{"cell":"8a2a1072b59ffff",
             "reports":[["0102030405060708",{"level":200,"ts_ms":1000}]]}]}"#,
    )
    .unwrap();

    let book = ZoneBook::load(&home, zones::DEFAULT_RESOLUTION)
        .expect("a stale zone cache must degrade, not refuse to load");
    let cell = u64::from_str_radix("8a2a1072b59ffff", 16).unwrap();
    assert!(book.get(cell).is_none(), "unreadable votes are dropped, not invented");

    // Quarantined rather than deleted: it is the only evidence of what went wrong, and
    // this codebase does not destroy a user's file to make an error go away.
    assert!(
        home.join("zones.json.bad").exists(),
        "the unreadable file should be kept for inspection"
    );
}
