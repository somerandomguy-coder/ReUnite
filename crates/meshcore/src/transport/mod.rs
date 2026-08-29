//! Link layer abstraction.
//!
//! The mesh core never talks to a radio directly - it hands opaque frames to a
//! `Transport` and receives frames back. Phase 1 ships the UDP transport, which runs the
//! mesh over any shared Wi-Fi link (a router with no internet, a phone hotspot, or a
//! laptop-hosted ad-hoc network). Phase 2's BLE / Wi-Fi Direct adapters implement this
//! same trait and drop in underneath the identical routing, crypto and CLI code.

pub mod udp;

#[cfg(target_os = "linux")]
pub mod ble_linux;

use std::net::SocketAddr;

use anyhow::Result;
use async_trait::async_trait;

pub use udp::{UdpConfig, UdpTransport};

#[cfg(target_os = "linux")]
pub use ble_linux::BleLinuxTransport;


#[async_trait]
pub trait Transport: Send + Sync {
    /// Push a frame to everyone in radio range.
    async fn send_broadcast(&self, frame: &[u8]) -> Result<()>;
    /// Push a frame to one known neighbour (used when a route is known).
    async fn send_to(&self, frame: &[u8], addr: SocketAddr) -> Result<()>;
    /// Await the next frame. Returns the raw bytes and the link address it came from.
    async fn recv(&self) -> Result<(Vec<u8>, SocketAddr)>;
    /// Human readable description for the banner.
    fn describe(&self) -> String;
}
