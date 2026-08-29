//! A transport whose radio lives outside Rust (plan.md §4 step 2.1).
//!
//! Mobile operating systems will not let a Rust library own the Bluetooth stack: iOS
//! exposes CoreBluetooth only to the app process, and Android's BLE APIs are tied to the
//! Activity lifecycle and its permission model. `plan.md` calls this out and specifies
//! native plugins, with the UI passing raw packets through to the core.
//!
//! So this `Transport` has no I/O of its own. It is a pair of queues:
//!
//! ```text
//!   node -> send_broadcast/send_to -> outbound queue -> platform drains it -> radio
//!   radio -> platform -> inject()  -> inbound channel -> recv() -> node
//! ```
//!
//! Everything above it - routing, dedupe, crypto, the node actor - is reused untouched,
//! which is the whole point of the `Transport` seam.
//!
//! ## Link addresses
//!
//! `Transport` addresses links by `SocketAddr`, which a Bluetooth device is not. Rather
//! than refactor the router now (that is Phase 3's `LinkAddr` work, deviation D1), this
//! keeps a bijection between the platform's device id string and a synthetic loopback
//! address. The router never inspects the address, it only hands it back to us, so the
//! substitution is invisible above this file.

use std::collections::{HashMap, VecDeque};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::sync::Mutex as AsyncMutex;

use super::Transport;

/// One frame the platform still has to put on the air.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outbound {
    pub frame: Vec<u8>,
    /// `None` means "send to everyone in range"; `Some(id)` names one connected device.
    pub to: Option<String>,
}

/// Maps platform device ids to the synthetic `SocketAddr`s the router stores.
#[derive(Default)]
struct LinkTable {
    to_addr: HashMap<String, SocketAddr>,
    to_device: HashMap<SocketAddr, String>,
    next: u32,
}

impl LinkTable {
    fn addr_for(&mut self, device: &str) -> SocketAddr {
        if let Some(addr) = self.to_addr.get(device) {
            return *addr;
        }
        // 127.0.0.0/8 is enormous and never routed off the machine, so a synthetic
        // address here can never collide with a real peer on a real network.
        self.next = self.next.wrapping_add(1);
        let octets = self.next.to_be_bytes();
        let addr = SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(127, octets[1], octets[2], octets[3].max(1)),
            1,
        ));
        self.to_addr.insert(device.to_string(), addr);
        self.to_device.insert(addr, device.to_string());
        addr
    }

    fn device_for(&self, addr: &SocketAddr) -> Option<String> {
        self.to_device.get(addr).cloned()
    }

    fn forget(&mut self, device: &str) {
        if let Some(addr) = self.to_addr.remove(device) {
            self.to_device.remove(&addr);
        }
    }
}

pub struct ExternalTransport {
    inbound_tx: mpsc::Sender<(Vec<u8>, SocketAddr)>,
    inbound_rx: AsyncMutex<mpsc::Receiver<(Vec<u8>, SocketAddr)>>,
    outbound: Mutex<VecDeque<Outbound>>,
    links: Mutex<LinkTable>,
    /// Last signal strength the platform reported per device.
    ///
    /// Keyed by device id rather than node id on purpose: the scanner sees an
    /// advertisement long before we know which node is behind it, and a reading we
    /// cannot yet attribute is still worth keeping until the first frame names it.
    rssi: Mutex<HashMap<String, i16>>,
    label: String,
    /// Bound on the outbound backlog. If the platform stops draining - the radio is off,
    /// Bluetooth permission was refused - the queue must not grow without limit.
    capacity: usize,
}

impl ExternalTransport {
    pub fn new(label: impl Into<String>) -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(512);
        Self {
            inbound_tx,
            inbound_rx: AsyncMutex::new(inbound_rx),
            outbound: Mutex::new(VecDeque::new()),
            links: Mutex::new(LinkTable::default()),
            rssi: Mutex::new(HashMap::new()),
            label: label.into(),
            capacity: 256,
        }
    }

    /// Hand the core a frame that arrived over the radio, from `device`.
    pub fn inject(&self, frame: Vec<u8>, device: &str) -> Result<()> {
        let addr = self
            .links
            .lock()
            .map_err(|_| anyhow!("link table poisoned"))?
            .addr_for(device);
        self.inbound_tx
            .try_send((frame, addr))
            .map_err(|e| anyhow!("inbound queue full or closed: {e}"))
    }

    /// Take the next frame the platform should transmit, if any.
    pub fn take_outbound(&self) -> Option<Outbound> {
        self.outbound.lock().ok()?.pop_front()
    }

    /// How many frames are waiting to go out. Exposed so the platform layer and tests can
    /// see backpressure rather than guess at it.
    pub fn pending(&self) -> usize {
        self.outbound.lock().map(|q| q.len()).unwrap_or(0)
    }

    /// Record the signal strength the scanner saw for one device.
    pub fn note_rssi(&self, device: &str, rssi: i16) {
        if let Ok(mut map) = self.rssi.lock() {
            map.insert(device.to_string(), rssi);
        }
    }

    /// A device disconnected. Drop its address mapping so a later reconnection gets a
    /// fresh one rather than inheriting a stale route.
    pub fn peer_lost(&self, device: &str) {
        if let Ok(mut links) = self.links.lock() {
            links.forget(device);
        }
        if let Ok(mut map) = self.rssi.lock() {
            map.remove(device);
        }
    }

    fn push(&self, item: Outbound) -> Result<()> {
        let mut queue = self
            .outbound
            .lock()
            .map_err(|_| anyhow!("outbound queue poisoned"))?;
        if queue.len() >= self.capacity {
            // Drop the oldest: in a mesh, a stale frame is worth less than a fresh one,
            // and every packet class here is either re-sent on a timer (beacons, zone
            // gossip) or retried from the outbox (direct messages).
            queue.pop_front();
        }
        queue.push_back(item);
        Ok(())
    }
}

#[async_trait]
impl Transport for ExternalTransport {
    async fn send_broadcast(&self, frame: &[u8]) -> Result<()> {
        self.push(Outbound {
            frame: frame.to_vec(),
            to: None,
        })
    }

    async fn send_to(&self, frame: &[u8], addr: SocketAddr) -> Result<()> {
        let device = self
            .links
            .lock()
            .map_err(|_| anyhow!("link table poisoned"))?
            .device_for(&addr);
        // An address we have never seen means the route is stale; fall back to a
        // broadcast rather than dropping the frame, exactly as flooding would.
        self.push(Outbound {
            frame: frame.to_vec(),
            to: device,
        })
    }

    async fn recv(&self) -> Result<(Vec<u8>, SocketAddr)> {
        let mut rx = self.inbound_rx.lock().await;
        rx.recv().await.ok_or_else(|| anyhow!("transport closed"))
    }

    fn describe(&self) -> String {
        self.label.clone()
    }

    fn rssi_for(&self, addr: &SocketAddr) -> Option<i16> {
        let device = self.links.lock().ok()?.device_for(addr)?;
        self.rssi.lock().ok()?.get(&device).copied()
    }
}
