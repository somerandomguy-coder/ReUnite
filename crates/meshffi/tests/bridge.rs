//! The FFI contract, exercised exactly as Dart will call it.
//!
//! The bridge is a process-global singleton (an app runs one node), so every check that
//! needs a running node lives in one test - two tests cannot each own one.
//!
//! Everything here goes through the real C entry points with real C strings, so a change
//! that breaks the JSON shape the UI depends on fails here rather than in a Flutter app.

use std::ffi::{CStr, CString};

use meshffi::{
    mesh_ble_drain, mesh_ble_inject, mesh_ble_peer_lost, mesh_command, mesh_free,
    mesh_poll_event, mesh_start, mesh_status_table, mesh_stop,
};

fn call(f: unsafe extern "C" fn(*const std::ffi::c_char) -> *mut std::ffi::c_char, json: &str) -> serde_json::Value {
    let input = CString::new(json).unwrap();
    unsafe {
        let raw = f(input.as_ptr());
        assert!(!raw.is_null(), "bridge returned null for {json}");
        let text = CStr::from_ptr(raw).to_str().unwrap().to_owned();
        mesh_free(raw);
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("bad json {text}: {e}"))
    }
}

#[test]
fn the_bridge_starts_a_node_and_answers_every_ui_command() {
    let home = std::env::temp_dir().join(format!("meshffi-test-{}", std::process::id()));
    let config = serde_json::json!({
        "home": home.to_string_lossy(),
        "name": "phone",
        "port": 47912u16,
        "multicast": false,
        "broadcast": false,
        "battery": 64u8,
    });

    let started = call(mesh_start, &config.to_string());
    assert_eq!(started["type"], "whoami", "start returns whoami, got {started}");
    let me = started["whoami"]["id"].as_str().unwrap().to_string();
    assert_eq!(me.len(), 16, "node id is 16 hex chars");
    assert_eq!(started["whoami"]["battery"], 64);
    assert_eq!(started["whoami"]["network"], "default");

    // Starting twice (a Flutter hot restart) must not bind the port twice.
    let again = call(mesh_start, &config.to_string());
    assert_eq!(again["whoami"]["id"], me.as_str());

    // Every screen's query returns its documented shape.
    assert!(call(mesh_command, r#"{"cmd":"peers"}"#)["peers"].is_array());
    assert!(call(mesh_command, r#"{"cmd":"routes"}"#)["routes"].is_array());
    assert!(call(mesh_command, r#"{"cmd":"heatmap"}"#)["zones"].is_array());
    assert!(call(mesh_command, r#"{"cmd":"history"}"#)["messages"].is_array());
    let nets = call(mesh_command, r#"{"cmd":"networks"}"#);
    assert_eq!(nets["networks"][0]["name"], "default");

    // The emergency features the UI puts behind buttons.
    call(mesh_command, r#"{"cmd":"set_location","lat":10.7769,"lon":106.7009}"#);
    assert_eq!(call(mesh_command, r#"{"cmd":"set_status","code":2}"#)["type"], "ok");
    assert_eq!(call(mesh_command, r#"{"cmd":"sos","on":true}"#)["type"], "ok");
    let zone = call(
        mesh_command,
        r#"{"cmd":"report_zone","lat":10.7769,"lon":106.7009,"verdict":"unsafe","radius_m":750}"#,
    );
    assert_eq!(zone["type"], "ok");

    let heat = call(mesh_command, r#"{"cmd":"heatmap"}"#);
    assert_eq!(heat["zones"][0]["verdict"], "unsafe");
    assert_eq!(heat["zones"][0]["radius_m"], 750);
    assert_eq!(heat["zones"][0]["unsafe_votes"], 1);
    assert_eq!(heat["zones"][0]["safe_votes"], 0);
    assert_eq!(heat["zones"][0]["mine"], true);

    // The bridge must refuse a verdict it does not understand rather than guessing one -
    // a guess here paints a real street the wrong colour.
    assert_eq!(
        call(
            mesh_command,
            r#"{"cmd":"report_zone","lat":10.0,"lon":106.0,"verdict":"maybe","radius_m":100}"#,
        )["type"],
        "error",
    );

    let who = call(mesh_command, r#"{"cmd":"whoami"}"#);
    assert_eq!(who["whoami"]["sos"], true);
    assert_eq!(who["whoami"]["status"], 2);

    // A private network round-trip, since the Networks screen drives it.
    assert_eq!(call(mesh_command, r#"{"cmd":"create_network","name":"rescue"}"#)["type"], "ok");
    let nets = call(mesh_command, r#"{"cmd":"networks"}"#);
    assert!(nets["networks"].as_array().unwrap().iter().any(|n| n["name"] == "rescue"));

    // Errors come back as data, never as a crash across the FFI boundary.
    assert_eq!(call(mesh_command, r#"{"cmd":"nonsense"}"#)["type"], "error");
    assert_eq!(call(mesh_command, r#"{"cmd":"direct"}"#)["type"], "error");
    assert_eq!(call(mesh_command, "not json at all")["type"], "error");
    // Creating it twice is an error, not a silent second network.
    assert_eq!(call(mesh_command, r#"{"cmd":"create_network","name":"rescue"}"#)["type"], "error");

    // ---- the event drain, which is how the UI receives everything ----
    // The Flutter UI polls on a timer from the UI thread and issues commands from that
    // same thread, so a non-blocking drain has to coexist with in-flight commands.
    let mut seen = Vec::new();
    for _ in 0..200 {
        let raw = mesh_poll_event(0);
        if raw.is_null() {
            // Keep issuing commands between drains - this is the real UI pattern.
            call(mesh_command, r#"{"cmd":"peers"}"#);
            continue;
        }
        unsafe {
            let text = CStr::from_ptr(raw).to_str().unwrap().to_owned();
            mesh_free(raw);
            let v: serde_json::Value = serde_json::from_str(&text).unwrap();
            seen.push(v["type"].as_str().unwrap().to_string());
        }
        if seen.iter().any(|t| t == "context") {
            break;
        }
    }
    assert!(
        seen.iter().any(|t| t == "context"),
        "expected a context event from --create-network, saw {seen:?}"
    );

    // ---- the Bluetooth transport ----
    // Switching radio means stopping one node and starting another in this process,
    // which is exactly what the app does when the user changes transport.
    assert!(mesh_stop(), "a node was running and should have stopped");
    assert_eq!(
        call(mesh_command, r#"{"cmd":"peers"}"#)["type"],
        "error",
        "commands must fail cleanly once the node is stopped"
    );

    let ble_home = std::env::temp_dir().join(format!("meshffi-ble-{}", std::process::id()));
    let ble_config = serde_json::json!({
        "home": ble_home.to_string_lossy(),
        "name": "phone-ble",
        "transport": "ble",
        "battery": 33u8,
    });
    let started = call(mesh_start, &ble_config.to_string());
    assert_eq!(started["type"], "whoami");
    assert!(
        started["whoami"]["transport"]
            .as_str()
            .unwrap()
            .contains("ble"),
        "transport should report the radio, got {}",
        started["whoami"]["transport"]
    );

    // The node beacons whether or not a radio is listening, so frames queue up for the
    // platform to collect. That queue is the whole contract with Kotlin and Swift.
    let mut queued: Vec<serde_json::Value> = Vec::new();
    for _ in 0..40 {
        let batch = unsafe {
            let raw = mesh_ble_drain();
            let text = CStr::from_ptr(raw).to_str().unwrap().to_owned();
            mesh_free(raw);
            serde_json::from_str::<Vec<serde_json::Value>>(&text).unwrap()
        };
        queued.extend(batch);
        if !queued.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(!queued.is_empty(), "the node should have queued a beacon for the radio");
    let first = &queued[0];
    assert!(first["to"].is_null(), "a beacon is a broadcast, so it names no device");
    let frame_hex = first["frame"].as_str().unwrap();
    assert!(!frame_hex.is_empty() && frame_hex.len() % 2 == 0, "frame is hex");

    // Feeding a frame back in is what the Kotlin/Swift layer does on every BLE receive.
    // Our own frame is rejected upstream as a self-echo, which is correct: the point
    // here is that the inject path accepts and parses it without error.
    let injected = call(
        mesh_ble_inject,
        &serde_json::json!({"frame": frame_hex, "from": "AA:BB:CC:DD:EE:FF"}).to_string(),
    );
    assert_eq!(injected["type"], "ok");

    // Malformed input is an error, never a crash across the boundary.
    assert_eq!(
        call(mesh_ble_inject, r#"{"frame":"zzzz","from":"x"}"#)["type"],
        "error"
    );
    assert_eq!(call(mesh_ble_inject, r#"{"from":"x"}"#)["type"], "error");

    unsafe {
        let device = CString::new("AA:BB:CC:DD:EE:FF").unwrap();
        mesh_ble_peer_lost(device.as_ptr());
    }

    assert!(mesh_stop());
    assert!(!mesh_stop(), "stopping twice is a no-op, not a crash");
}

/// The UI builds its panic buttons from this table, so what matters is that the bridge
/// reproduces `meshcore::status::TABLE` faithfully - not that the table has any particular
/// contents. Asserting the contents is how this test came to fail when the table was
/// deliberately cut from seven codes to three: it was testing a product decision, which
/// belongs in a product decision's own review, not in the FFI contract.
#[test]
fn the_status_table_comes_from_the_core() {
    unsafe {
        let raw = mesh_status_table();
        let text = CStr::from_ptr(raw).to_str().unwrap().to_owned();
        mesh_free(raw);
        let rows: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();

        assert_eq!(
            rows.len(),
            meshcore::status::TABLE.len(),
            "the bridge must expose every code the core carries, and no others",
        );
        assert!(!rows.is_empty(), "a UI with no panic buttons is not a UI");

        for (row, expected) in rows.iter().zip(meshcore::status::TABLE) {
            assert_eq!(row["code"], expected.code);
            assert_eq!(row["name"], expected.name);
            assert_eq!(row["text"], expected.text);
        }

        let mut codes: Vec<u64> = rows.iter().map(|r| r["code"].as_u64().unwrap()).collect();
        codes.sort_unstable();
        let unique = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), unique, "two statuses share one wire code");
    }
}

#[test]
fn polling_with_no_node_running_returns_null_instead_of_crashing() {
    // A UI that polls before start, or after stop, must get a benign answer.
    let raw = mesh_poll_event(1);
    if !raw.is_null() {
        unsafe { mesh_free(raw) };
    }
}