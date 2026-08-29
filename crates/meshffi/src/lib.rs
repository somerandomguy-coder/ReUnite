//! C ABI bridge from the mesh core to the Flutter UI (plan.md §4 Phase 2, step 2.1).
//!
//! The whole surface is five functions and a JSON string in each direction. `meshcore`'s
//! `NodeHandle` already exposes exactly one way in (`Command`) and one way out (`Event`),
//! so the bridge is a translation layer and nothing more - no protocol logic lives here,
//! and none should.
//!
//! ```text
//!   Dart  --mesh_start(config json)-->  spawn Node on a tokio runtime
//!   Dart  --mesh_command(cmd json)-->   NodeHandle::call  --> reply json
//!   Dart  <--mesh_poll_event(ms)-----   the Event channel  --> event json
//! ```
//!
//! `mesh_poll_event` blocks for up to `timeout_ms`, so Dart drives it from a background
//! isolate and never spins.

mod dto;

use std::ffi::{c_char, CStr, CString};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use anyhow::{anyhow, Result};
use meshcore::node::{Command, Event, Node, NodeConfig, NodeHandle};
use meshcore::store::resolve_home;
use meshcore::transport::{ExternalTransport, MultiTransport, Transport, UdpConfig, UdpTransport};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

use dto::{CommandDto, EventDto, ReplyDto, StartConfig};

struct Bridge {
    runtime: Runtime,
    handle: NodeHandle,
    events: Mutex<mpsc::Receiver<Event>>,
    /// Present only when the node was started on the `ble` transport. The platform's
    /// Bluetooth layer drains and feeds this; see `transport/external.rs`.
    ble: Option<std::sync::Arc<ExternalTransport>>,
}

/// The running node, if any.
///
/// An `RwLock<Option<Arc<..>>>` rather than a `OnceLock`, because switching transport -
/// Wi-Fi to Bluetooth and back - has to stop one node and start another in the same
/// process. Callers clone the `Arc` out under a short read lock and release it before
/// doing any blocking work, so a slow command cannot stall the BLE drain.
fn bridge_slot() -> &'static RwLock<Option<Arc<Bridge>>> {
    static BRIDGE: OnceLock<RwLock<Option<Arc<Bridge>>>> = OnceLock::new();
    BRIDGE.get_or_init(|| RwLock::new(None))
}

fn bridge() -> Option<Arc<Bridge>> {
    bridge_slot().read().ok()?.clone()
}

// ------------------------------------------------------------------ ffi helpers

/// Move a Rust string across the boundary. Dart must hand the pointer back to
/// `mesh_free` - anything else leaks.
fn out(text: String) -> *mut c_char {
    match CString::new(text) {
        Ok(s) => s.into_raw(),
        Err(_) => CString::new(r#"{"type":"error","message":"interior nul byte"}"#)
            .expect("literal has no nul")
            .into_raw(),
    }
}

fn err_json(message: impl std::fmt::Display) -> *mut c_char {
    out(
        serde_json::to_string(&ReplyDto::Error {
            message: message.to_string(),
        })
        .unwrap_or_else(|_| r#"{"type":"error","message":"unprintable error"}"#.to_string()),
    )
}

/// # Safety
/// `ptr` must be a NUL-terminated C string that stays valid for this call.
unsafe fn as_str<'a>(ptr: *const c_char) -> Result<&'a str> {
    if ptr.is_null() {
        return Err(anyhow!("null pointer"));
    }
    Ok(CStr::from_ptr(ptr).to_str()?)
}

// ---------------------------------------------------------------------- lifecycle

/// Start the node. Returns a `ReplyDto` JSON string: `whoami` on success, `error`
/// otherwise. Calling it twice is a no-op that re-reports the running node, which is what
/// a Flutter hot restart does.
///
/// # Safety
/// `config_json` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn mesh_start(config_json: *const c_char) -> *mut c_char {
    let text = match as_str(config_json) {
        Ok(t) => t,
        Err(e) => return err_json(e),
    };
    match start_inner(text) {
        Ok(json) => out(json),
        Err(e) => err_json(e),
    }
}

fn start_inner(config_json: &str) -> Result<String> {
    if bridge().is_some() {
        // Already running (hot restart): report the live node rather than binding twice.
        return command_inner(r#"{"cmd":"whoami"}"#);
    }
    let cfg: StartConfig = serde_json::from_str(config_json)?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;

    let home = resolve_home(Some(PathBuf::from(&cfg.home)))?;
    let seeds = cfg
        .peers
        .iter()
        .filter_map(|p| p.parse().ok())
        .collect::<Vec<_>>();

    let name = cfg.name.clone();
    let battery = cfg.battery;

    // Phase 2D: every radio at once, not a choice.
    //
    // `transport` is now a *hint* about which radios to try, kept only so the CLI and the
    // tests can still pin one. The app passes "all" and gets whatever this device has.
    // The picker it replaced was a configuration question put to somebody in an
    // emergency, and its correct answer was always "all of them".
    let want = cfg.transport.to_ascii_lowercase();
    let want_ble = want == "ble" || want == "all";
    let want_udp = want == "udp" || want == "all";

    // The BLE handle comes back out of the async block rather than being captured, so
    // the platform layer can keep feeding it after start returns.
    let (handle, events, ble_transport) = runtime.block_on(async move {
        let mut ble_transport: Option<std::sync::Arc<ExternalTransport>> = None;
        let mut radios: Vec<std::sync::Arc<dyn Transport>> = Vec::new();
        let mut failures: Vec<String> = Vec::new();

        if want_ble {
            // The radio itself is Kotlin or Swift; Rust only queues frames for it. There
            // is nothing here that can fail, so a phone always has this one - whether the
            // radio behind it is on is the platform's business, reported separately.
            let external = std::sync::Arc::new(ExternalTransport::new("ble (native radio)"));
            ble_transport = Some(external.clone());
            radios.push(external as std::sync::Arc<dyn Transport>);
        }

        if want_udp {
            // Binding can genuinely fail - the port is taken, or the OS refuses. That
            // must cost this one radio, not the node: a phone with no Wi-Fi still meshes
            // over Bluetooth.
            match UdpTransport::bind(UdpConfig {
                port: cfg.port,
                group: std::net::Ipv4Addr::new(239, 42, 13, 7),
                multicast: cfg.multicast,
                broadcast: cfg.broadcast,
                seeds,
            }) {
                Ok(udp) => radios.push(std::sync::Arc::new(udp)),
                Err(e) => failures.push(format!("wi-fi: {e}")),
            }
        }

        if radios.is_empty() {
            return Err(anyhow!(
                "no radio could be started ({})",
                if failures.is_empty() {
                    "none were requested".to_string()
                } else {
                    failures.join("; ")
                }
            ));
        }

        let transport: std::sync::Arc<dyn Transport> = if radios.len() == 1 {
            radios.remove(0)
        } else {
            std::sync::Arc::new(MultiTransport::new(radios)?)
        };

        let mut node = NodeConfig::new(home);
        node.self_name = name;
        node.battery_override = battery;
        let (handle, events) = Node::spawn(node, transport)?;
        Ok::<_, anyhow::Error>((handle, events, ble_transport))
    })?;

    *bridge_slot()
        .write()
        .map_err(|_| anyhow!("bridge lock poisoned"))? = Some(Arc::new(Bridge {
        runtime,
        handle,
        events: Mutex::new(events),
        ble: ble_transport,
    }));
    command_inner(r#"{"cmd":"whoami"}"#)
}

/// Free a string returned by any function in this module.
///
/// # Safety
/// `ptr` must have come from this library and must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn mesh_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

// ----------------------------------------------------------------------- commands

/// Run one command. Returns a `ReplyDto` JSON string.
///
/// # Safety
/// `cmd_json` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn mesh_command(cmd_json: *const c_char) -> *mut c_char {
    let text = match as_str(cmd_json) {
        Ok(t) => t,
        Err(e) => return err_json(e),
    };
    match command_inner(text) {
        Ok(json) => out(json),
        Err(e) => err_json(e),
    }
}

fn command_inner(cmd_json: &str) -> Result<String> {
    let b = bridge().ok_or_else(|| anyhow!("mesh not started"))?;
    let dto: CommandDto = serde_json::from_str(cmd_json)?;
    let command = to_command(dto)?;
    let reply = b.runtime.block_on(b.handle.call(command))?;
    Ok(serde_json::to_string(&ReplyDto::from(reply))?)
}

fn need<T>(value: Option<T>, field: &str) -> Result<T> {
    value.ok_or_else(|| anyhow!("missing field '{field}'"))
}

fn to_command(d: CommandDto) -> Result<Command> {
    Ok(match d.cmd.as_str() {
        "broadcast" => Command::Broadcast(need(d.text, "text")?),
        "direct" => Command::Direct {
            target: need(d.target, "target")?,
            text: need(d.text, "text")?,
        },
        "create_network" => Command::CreateNetwork(need(d.name, "name")?),
        "invite" => Command::Invite {
            network: need(d.network, "network")?,
            user: need(d.user, "user")?,
        },
        "set_storing" => Command::SetStoring {
            network: need(d.network, "network")?,
            on: need(d.on, "on")?,
        },
        "kick" => Command::Kick(need(d.user, "user")?),
        "rename" => Command::Rename {
            user: need(d.user, "user")?,
            name: need(d.name, "name")?,
        },
        "switch" => Command::Switch(need(d.name, "name")?),
        "peers" => Command::Peers,
        "networks" => Command::Networks,
        "routes" => Command::Routes,
        "history" => Command::History(d.limit.unwrap_or(50)),
        "whoami" => Command::Whoami,
        "set_location" => Command::SetLocation {
            lat: need(d.lat, "lat")?,
            lon: need(d.lon, "lon")?,
        },
        "share_location" => Command::ShareLocation,
        "sos" => Command::Sos(need(d.on, "on")?),
        "set_status" => Command::SetStatus {
            code: need(d.code, "code")?,
        },
        "report_zone" => {
            let word = need(d.verdict, "verdict")?;
            Command::ReportZone {
                lat: need(d.lat, "lat")?,
                lon: need(d.lon, "lon")?,
                verdict: meshcore::zones::parse_verdict(&word)
                    .ok_or_else(|| anyhow!("verdict must be 'safe' or 'unsafe', not '{word}'"))?,
                radius_m: need(d.radius_m, "radius_m")?,
            }
        }
        "heatmap" => Command::Heatmap,
        other => return Err(anyhow!("unknown command '{other}'")),
    })
}

// ------------------------------------------------------------------------- events

/// Take the next event, or NULL when there is none. Returns an `EventDto` JSON string.
///
/// `timeout_ms == 0` is a **non-blocking** drain - that is what the Flutter UI uses, on a
/// timer, so it never blocks the UI thread waiting for a beacon. A non-zero timeout
/// blocks the calling thread for up to that long, which is only safe from a background
/// isolate or another native thread.
#[no_mangle]
pub extern "C" fn mesh_poll_event(timeout_ms: u64) -> *mut c_char {
    let Some(b) = bridge() else {
        return std::ptr::null_mut();
    };
    let Ok(mut rx) = b.events.lock() else {
        return std::ptr::null_mut();
    };
    let event = if timeout_ms == 0 {
        rx.try_recv().ok()
    } else {
        b.runtime
            .block_on(async {
                tokio::time::timeout(Duration::from_millis(timeout_ms), rx.recv()).await
            })
            .ok()
            .flatten()
    };
    match event {
        Some(event) => match serde_json::to_string(&EventDto::from(event)) {
            Ok(json) => out(json),
            Err(_) => std::ptr::null_mut(),
        },
        None => std::ptr::null_mut(),
    }
}

/// The status-code table, so the UI's panic buttons come from the core rather than being
/// re-typed in Dart and drifting out of sync.
#[no_mangle]
pub extern "C" fn mesh_status_table() -> *mut c_char {
    let rows: Vec<serde_json::Value> = meshcore::status::TABLE
        .iter()
        .map(|s| {
            serde_json::json!({ "code": s.code, "name": s.name, "text": s.text })
        })
        .collect();
    out(serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into()))
}

// ------------------------------------------------------------------ ble radio

/// Frames the platform's Bluetooth layer must transmit, drained in one call.
///
/// Returns a JSON array: `[{"frame":"<hex>","to":"<device id>"|null}, ...]`, where a null
/// `to` means "advertise/write to every connected peer". Empty array when there is
/// nothing to send, and `[]` too when the node is not on the BLE transport.
///
/// Draining in a batch rather than one at a time keeps the FFI chatter down: a node
/// beacons every three seconds but a burst of relayed traffic can queue many at once.
#[no_mangle]
pub extern "C" fn mesh_ble_drain() -> *mut c_char {
    let Some(b) = bridge() else {
        return out("[]".into());
    };
    let Some(ble) = b.ble.as_ref() else {
        return out("[]".into());
    };
    let mut items = Vec::new();
    // Bounded so one call cannot block the UI thread for an unbounded time.
    while items.len() < 64 {
        match ble.take_outbound() {
            Some(o) => items.push(serde_json::json!({
                "frame": hex::encode(&o.frame),
                "to": o.to,
            })),
            None => break,
        }
    }
    out(serde_json::to_string(&items).unwrap_or_else(|_| "[]".into()))
}

/// Hand the core a frame that arrived over Bluetooth.
///
/// Input: `{"frame":"<hex>","from":"<device id>"}`. The device id is whatever the
/// platform uses to name a peer - an Android MAC, an iOS peripheral UUID. It is opaque
/// here; the core only needs it to be stable for the life of a connection.
///
/// # Safety
/// `json` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn mesh_ble_inject(json: *const c_char) -> *mut c_char {
    let text = match as_str(json) {
        Ok(t) => t,
        Err(e) => return err_json(e),
    };
    match inject_inner(text) {
        Ok(()) => out(r#"{"type":"ok","message":"injected"}"#.into()),
        Err(e) => err_json(e),
    }
}

fn inject_inner(json: &str) -> Result<()> {
    let b = bridge().ok_or_else(|| anyhow!("mesh not started"))?;
    let ble = b
        .ble
        .as_ref()
        .ok_or_else(|| anyhow!("node is not running on the ble transport"))?;
    let value: serde_json::Value = serde_json::from_str(json)?;
    let frame_hex = value["frame"]
        .as_str()
        .ok_or_else(|| anyhow!("missing 'frame'"))?;
    let from = value["from"].as_str().ok_or_else(|| anyhow!("missing 'from'"))?;
    let frame = hex::decode(frame_hex)?;
    ble.inject(frame, from)
}

/// Record the signal strength the scanner saw for one device.
///
/// Input: `{"device":"<device id>","rssi":-57}`.
///
/// Kept separate from `mesh_ble_inject` because the two arrive at completely different
/// rates: a scanner reports RSSI several times a second for every device in range,
/// including ones that have never sent us a frame and may never connect.
///
/// # Safety
/// `json` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn mesh_ble_rssi(json: *const c_char) {
    let Ok(text) = as_str(json) else { return };
    let Some(b) = bridge() else { return };
    let Some(ble) = b.ble.as_ref() else { return };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    let (Some(device), Some(rssi)) = (value["device"].as_str(), value["rssi"].as_i64()) else {
        return;
    };
    ble.note_rssi(device, rssi.clamp(i16::MIN as i64, i16::MAX as i64) as i16);
}

/// A Bluetooth peer disconnected; drop its link mapping so a reconnection starts clean.
///
/// # Safety
/// `device` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn mesh_ble_peer_lost(device: *const c_char) {
    let Ok(text) = as_str(device) else { return };
    if let Some(b) = bridge() {
        if let Some(ble) = b.ble.as_ref() {
            ble.peer_lost(text);
        }
    }
}

/// Stop the running node and release its port or radio.
///
/// Needed to switch transport without killing the app. Returns true if a node was
/// actually stopped. The tokio runtime is shut down in the background because dropping
/// it inline would block on in-flight tasks, and this is called from the UI thread.
#[no_mangle]
pub extern "C" fn mesh_stop() -> bool {
    let taken = match bridge_slot().write() {
        Ok(mut slot) => slot.take(),
        Err(_) => return false,
    };
    match taken {
        Some(b) => {
            match Arc::try_unwrap(b) {
                Ok(owned) => owned.runtime.shutdown_background(),
                // Another thread is mid-call; dropping our reference is enough, and the
                // node dies with the last one.
                Err(_) => {}
            }
            true
        }
        None => false,
    }
}
