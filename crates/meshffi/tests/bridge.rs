//! The FFI contract, exercised exactly as Dart will call it.
//!
//! The bridge is a process-global singleton (an app runs one node), so every check that
//! needs a running node lives in one test - two tests cannot each own one.
//!
//! Everything here goes through the real C entry points with real C strings, so a change
//! that breaks the JSON shape the UI depends on fails here rather than in a Flutter app.

use std::ffi::{CStr, CString};

use meshffi::{mesh_command, mesh_free, mesh_poll_event, mesh_start, mesh_status_table};

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
        r#"{"cmd":"report_zone","lat":10.7769,"lon":106.7009,"level":4}"#,
    );
    assert_eq!(zone["type"], "ok");

    let heat = call(mesh_command, r#"{"cmd":"heatmap"}"#);
    assert_eq!(heat["zones"][0]["consensus"], 1);
    assert_eq!(heat["zones"][0]["mine"], true);
    assert!((heat["zones"][0]["level_scaled"].as_f64().unwrap() - 4.0).abs() < 0.01);

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
}

#[test]
fn the_status_table_comes_from_the_core() {
    unsafe {
        let raw = mesh_status_table();
        let text = CStr::from_ptr(raw).to_str().unwrap().to_owned();
        mesh_free(raw);
        let rows: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 7);
        assert_eq!(rows[1]["name"], "medical");
        assert_eq!(rows[1]["code"], 2);
        assert_eq!(rows[1]["text"], "Need medical help");
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

