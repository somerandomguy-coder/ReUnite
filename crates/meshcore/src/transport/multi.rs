//! Every radio at once (phase 2D).
//!
//! Phase 2 made the radio a choice: the app started on Bluetooth *or* Wi-Fi, and changing
//! it stopped and restarted the node. That is a configuration question put to somebody in
//! an emergency, and its correct answer is always "all of them" - the radios have
//! different range, different power cost and different failure modes, and the mesh is
//! strictly better on every one it can reach.
//!
//! So this is a `Transport` over a set of `Transport`s:
//!
//! ```text
//!   send_broadcast  -> every child
//!   send_to(addr)   -> the child that address arrived on, or every child if unknown
//!   recv            -> whichever child speaks first
//! ```
//!
//! Routing, crypto, dedupe and the node actor see one radio and do not change. A node id
//! reachable on two radios is still one peer: the router keys neighbours by `NodeId`, and
//! the duplicate suppression that already handles a flooded packet arriving twice handles
//! it arriving twice over different radios for free.
//!
//! ## One dead radio must never take down the node
//!
//! Every failure here is non-fatal. A phone with Bluetooth switched off still meshes over
//! Wi-Fi; a laptop with no BLE peripheral role still meshes over UDP. `send_broadcast`
//! succeeds if *any* child accepted the frame, and only reports an error when they all
//! refused - because at that point there is genuinely no radio left.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;

use super::Transport;

pub struct MultiTransport {
    children: Vec<Arc<dyn Transport>>,
    inbound: AsyncMutex<mpsc::Receiver<(Vec<u8>, SocketAddr, usize)>>,
    /// Which child a link address was last heard on, so a routed frame goes back out the
    /// way it came instead of being broadcast over every radio.
    owner: Mutex<HashMap<SocketAddr, usize>>,
    pumps: Vec<JoinHandle<()>>,
}

impl MultiTransport {
    /// Wrap one or more transports. Errors only if given none - a node with no radio at
    /// all is a configuration mistake, not a degraded mode.
    pub fn new(children: Vec<Arc<dyn Transport>>) -> Result<Self> {
        if children.is_empty() {
            return Err(anyhow!("a node needs at least one transport"));
        }
        let (tx, rx) = mpsc::channel(512);
        let mut pumps = Vec::new();

        // `Transport::recv` is a single-consumer await, and `select!` over a runtime-sized
        // set of them is awkward and easy to get wrong under cancellation. One pump task
        // per child feeding a shared channel is simpler and cannot lose a frame to a
        // cancelled branch.
        for (index, child) in children.iter().enumerate() {
            let child = child.clone();
            let tx = tx.clone();
            pumps.push(tokio::spawn(async move {
                loop {
                    match child.recv().await {
                        Ok((bytes, addr)) => {
                            if tx.send((bytes, addr, index)).await.is_err() {
                                return; // the node is gone
                            }
                        }
                        Err(_) => {
                            // A child that is failing must not spin. It also must not
                            // take the others down, so this is a pause, not a return.
                            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                        }
                    }
                }
            }));
        }

        Ok(Self {
            children,
            inbound: AsyncMutex::new(rx),
            owner: Mutex::new(HashMap::new()),
            pumps,
        })
    }

    /// How many radios are underneath. The UI shows this rather than a picker.
    pub fn len(&self) -> usize {
        self.children.len()
    }

    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// Each child's description, for the radio status panel.
    pub fn describe_each(&self) -> Vec<String> {
        self.children.iter().map(|c| c.describe()).collect()
    }

    fn child_for(&self, addr: &SocketAddr) -> Option<usize> {
        self.owner.lock().ok()?.get(addr).copied()
    }
}

impl Drop for MultiTransport {
    fn drop(&mut self) {
        for pump in &self.pumps {
            pump.abort();
        }
    }
}

#[async_trait]
impl Transport for MultiTransport {
    async fn send_broadcast(&self, frame: &[u8]) -> Result<()> {
        let mut sent = 0;
        let mut last: Option<anyhow::Error> = None;
        for child in &self.children {
            match child.send_broadcast(frame).await {
                Ok(()) => sent += 1,
                Err(e) => last = Some(e),
            }
        }
        if sent > 0 {
            return Ok(());
        }
        Err(last.unwrap_or_else(|| anyhow!("no transport accepted the frame")))
    }

    async fn send_to(&self, frame: &[u8], addr: SocketAddr) -> Result<()> {
        if let Some(index) = self.child_for(&addr) {
            if let Some(child) = self.children.get(index) {
                return child.send_to(frame, addr).await;
            }
        }
        // An address we have not seen belongs to no child in particular. Falling back to
        // a broadcast is what flooding would have done anyway, and is far better than
        // dropping a frame because a route outlived the link it was learned on.
        self.send_broadcast(frame).await
    }

    async fn recv(&self) -> Result<(Vec<u8>, SocketAddr)> {
        let mut rx = self.inbound.lock().await;
        let (bytes, addr, index) = rx
            .recv()
            .await
            .ok_or_else(|| anyhow!("all transports closed"))?;
        if let Ok(mut owner) = self.owner.lock() {
            owner.insert(addr, index);
        }
        Ok((bytes, addr))
    }

    fn describe(&self) -> String {
        self.describe_each().join(" + ")
    }

    fn rssi_for(&self, addr: &SocketAddr) -> Option<i16> {
        let index = self.child_for(addr)?;
        self.children.get(index)?.rssi_for(addr)
    }
}
