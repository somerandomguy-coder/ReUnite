//! C-FFI and Flutter FFI Bindings for meshcore actor.
//! Allows Flutter (Android/iOS/Desktop) to drive the native Rust P2P mesh network.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

use crate::node::{Command, Event, Node, NodeConfig, NodeHandle};
use crate::transport::udp::{UdpConfig, UdpTransport};

static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);
static HANDLE: Mutex<Option<NodeHandle>> = Mutex::new(None);
static EVENT_RX: Mutex<Option<mpsc::Receiver<Event>>> = Mutex::new(None);

/// Initialize the native Rust mesh node on Android / iOS / Desktop
///
/// # Safety
/// `home_path` and `name` must be valid, null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn mesh_node_init(home_path: *const c_char, name: *const c_char) -> *mut c_char {
    if home_path.is_null() {
        return CString::new(r#"{"status":"error","message":"null_home_path"}"#)
            .unwrap()
            .into_raw();
    }

    let home_str = match CStr::from_ptr(home_path).to_str() {
        Ok(s) => s,
        Err(_) => return CString::new(r#"{"status":"error","message":"invalid_utf8"}"#).unwrap().into_raw(),
    };

    let name_str = if name.is_null() {
        None
    } else {
        CStr::from_ptr(name).to_str().ok().map(|s| s.to_string())
    };

    let rt = match Runtime::new() {
        Ok(r) => r,
        Err(e) => return CString::new(format!(r#"{{"status":"error","message":"{}"}}"#, e)).unwrap().into_raw(),
    };

    let home_dir = PathBuf::from(home_str);
    let mut config = NodeConfig::new(home_dir);
    if let Some(n) = name_str {
        config.self_name = Some(n);
    }
    config.hello_interval = Duration::from_secs(2);

    let res = rt.block_on(async {
        let mut udp_cfg = UdpConfig::default();
        udp_cfg.port = 0; // Dynamic ephemeral port for mobile/desktop FFI
        let transport = UdpTransport::bind(udp_cfg)?;
        let (handle, event_rx) = Node::spawn(config, Arc::new(transport))?;
        let node_id = handle.id.to_string();

        Ok::<(NodeHandle, mpsc::Receiver<Event>, String), anyhow::Error>((handle, event_rx, node_id))
    });

    let (handle, event_rx, node_id) = match res {
        Ok(res) => res,
        Err(e) => return CString::new(format!(r#"{{"status":"error","message":"{}"}}"#, e)).unwrap().into_raw(),
    };

    if let Ok(mut g) = RUNTIME.lock() {
        *g = Some(rt);
    }
    if let Ok(mut g) = HANDLE.lock() {
        *g = Some(handle);
    }
    if let Ok(mut g) = EVENT_RX.lock() {
        *g = Some(event_rx);
    }

    let resp = format!(r#"{{"status":"ok","nodeId":"{}"}}"#, node_id);
    CString::new(resp).unwrap().into_raw()
}

/// Send a Command string to the active native Rust mesh actor.
///
/// # Safety
/// `cmd_str` must be a valid, null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn mesh_node_send_command(cmd_str: *const c_char) -> *mut c_char {
    if cmd_str.is_null() {
        return CString::new(r#"{"status":"error","message":"null_cmd"}"#).unwrap().into_raw();
    }

    let input = match CStr::from_ptr(cmd_str).to_str() {
        Ok(s) => s,
        Err(_) => return CString::new(r#"{"status":"error","message":"invalid_utf8"}"#).unwrap().into_raw(),
    };

    let handle_guard = HANDLE.lock().unwrap();
    let handle = match handle_guard.as_ref() {
        Some(h) => h.clone(),
        None => return CString::new(r#"{"status":"error","message":"node_not_initialized"}"#).unwrap().into_raw(),
    };
    drop(handle_guard);

    let runtime_guard = RUNTIME.lock().unwrap();
    let rt = match runtime_guard.as_ref() {
        Some(r) => r,
        None => return CString::new(r#"{"status":"error","message":"runtime_not_initialized"}"#).unwrap().into_raw(),
    };

    let cmd = if input.starts_with("sos:on") {
        Command::Sos(true)
    } else if input.starts_with("sos:off") {
        Command::Sos(false)
    } else if input.starts_with("msg:") {
        Command::Broadcast(input[4..].to_string())
    } else if input.starts_with("network:") {
        Command::CreateNetwork(input[8..].to_string())
    } else {
        Command::Broadcast(input.to_string())
    };

    let res = rt.block_on(async {
        handle.call(cmd).await
    });

    match res {
        Ok(reply) => {
            let resp = format!(r#"{{"status":"ok","reply":"{:?}"}}"#, reply);
            CString::new(resp).unwrap().into_raw()
        }
        Err(e) => {
            let resp = format!(r#"{{"status":"error","message":"{}"}}"#, e);
            CString::new(resp).unwrap().into_raw()
        }
    }
}

/// Poll for incoming Events (Chat messages, SOS alerts, discovered Peers) from the Rust mesh node.
#[no_mangle]
pub extern "C" fn mesh_node_poll_event() -> *mut c_char {
    let mut rx_guard = EVENT_RX.lock().unwrap();
    let rx = match rx_guard.as_mut() {
        Some(r) => r,
        None => return CString::new(r#"{"status":"empty"}"#).unwrap().into_raw(),
    };

    match rx.try_recv() {
        Ok(event) => {
            let json = match event {
                Event::Chat { network, from_id, from, text, hops } => {
                    format!(r#"{{"type":"chat","network":"{}","fromId":"{}","from":"{}","text":"{}","hops":{}}}"#, network, from_id, from, text, hops)
                }
                Event::SosRaised { id, display, gps, distance_m } => {
                    let (lat, lon) = gps.map(|g| (g.lat, g.lon)).unwrap_or((0.0, 0.0));
                    format!(r#"{{"type":"sos_raised","fromId":"{}","from":"{}","lat":{},"lon":{},"distance":{}}}"#, id, display, lat, lon, distance_m.unwrap_or(0.0))
                }
                Event::SosCleared { id, display } => {
                    format!(r#"{{"type":"sos_cleared","fromId":"{}","from":"{}"}}"#, id, display)
                }
                Event::PeerJoined { id, display } => {
                    format!(r#"{{"type":"peer_joined","fromId":"{}","from":"{}"}}"#, id, display)
                }
                Event::PeerLost { id, display } => {
                    format!(r#"{{"type":"peer_lost","fromId":"{}","from":"{}"}}"#, id, display)
                }
                _ => format!(r#"{{"type":"event","detail":"{:?}"}}"#, event),
            };
            CString::new(json).unwrap().into_raw()
        }
        Err(_) => CString::new(r#"{"status":"empty"}"#).unwrap().into_raw(),
    }
}

/// Free C-string pointers allocated by Rust
///
/// # Safety
/// `s` must be a pointer allocated by `CString::into_raw`.
#[no_mangle]
pub unsafe extern "C" fn mesh_string_free(s: *mut c_char) {
    if !s.is_null() {
        let _ = CString::from_raw(s);
    }
}
