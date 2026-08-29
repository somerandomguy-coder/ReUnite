//! UDP transport: IPv4 multicast for discovery plus unicast for routed traffic.
//!
//! Discovery is multicast (default `239.42.13.7:47474`) with a limited-broadcast copy for
//! networks that filter multicast. Explicit `--peer host:port` seeds cover the cases where
//! neither works: two nodes on one laptop, or two laptops joined over a link that blocks
//! multicast.

use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Mutex;

use anyhow::{Context, Result};
use async_trait::async_trait;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;

use super::Transport;

#[derive(Clone, Debug)]
pub struct UdpConfig {
    pub port: u16,
    pub group: Ipv4Addr,
    pub multicast: bool,
    pub broadcast: bool,
    /// Peers we always send discovery frames to, regardless of multicast reachability.
    pub seeds: Vec<SocketAddr>,
}

impl Default for UdpConfig {
    fn default() -> Self {
        Self {
            port: 47474,
            group: Ipv4Addr::new(239, 42, 13, 7),
            multicast: true,
            broadcast: true,
            seeds: Vec::new(),
        }
    }
}

pub struct UdpTransport {
    socket: UdpSocket,
    config: UdpConfig,
    local: SocketAddr,
    /// Addresses we have actually heard from. Campus and hotel Wi-Fi often drop
    /// multicast, so once a single frame gets through by any means we keep talking to
    /// that address directly. Duplicate copies are cheap: the router's dedupe cache
    /// throws away every repeat of a packet id.
    links: Mutex<HashSet<SocketAddr>>,
}

impl UdpTransport {
    pub fn bind(config: UdpConfig) -> Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        #[cfg(unix)]
        socket.set_reuse_port(true)?;
        socket.set_nonblocking(true)?;
        let bind_addr: SocketAddr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, config.port).into();
        socket
            .bind(&bind_addr.into())
            .with_context(|| format!("binding UDP port {}", config.port))?;
        if config.broadcast {
            socket.set_broadcast(true)?;
        }
        if config.multicast {
            socket.set_multicast_loop_v4(true)?;
            socket
                .join_multicast_v4(&config.group, &Ipv4Addr::UNSPECIFIED)
                .with_context(|| format!("joining multicast group {}", config.group))?;
        }
        let std_socket: std::net::UdpSocket = socket.into();
        let socket = UdpSocket::from_std(std_socket)?;
        let local = socket.local_addr()?;
        Ok(Self {
            socket,
            config,
            local,
            links: Mutex::new(HashSet::new()),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local
    }

    pub fn seeds(&self) -> &[SocketAddr] {
        &self.config.seeds
    }
}

#[async_trait]
impl Transport for UdpTransport {
    async fn send_broadcast(&self, frame: &[u8]) -> Result<()> {
        let mut targets: Vec<SocketAddr> = Vec::new();
        if self.config.multicast {
            targets.push(SocketAddrV4::new(self.config.group, self.config.port).into());
        }
        if self.config.broadcast {
            targets.push(SocketAddrV4::new(Ipv4Addr::BROADCAST, self.config.port).into());
        }
        targets.extend(self.config.seeds.iter().copied());
        if let Ok(links) = self.links.lock() {
            targets.extend(links.iter().copied());
        }
        // A dead interface (Wi-Fi just dropped) must not kill the node: try them all.
        for target in targets {
            let _ = self.socket.send_to(frame, target).await;
        }
        Ok(())
    }

    async fn send_to(&self, frame: &[u8], addr: SocketAddr) -> Result<()> {
        self.socket.send_to(frame, addr).await?;
        Ok(())
    }

    async fn recv(&self) -> Result<(Vec<u8>, SocketAddr)> {
        let mut buf = vec![0u8; 64 * 1024];
        let (len, from) = self.socket.recv_from(&mut buf).await?;
        buf.truncate(len);
        if from != self.local {
            if let Ok(mut links) = self.links.lock() {
                links.insert(from);
            }
        }
        Ok((buf, from))
    }

    fn describe(&self) -> String {
        let mut parts = vec![format!("udp/{}", self.local)];
        if self.config.multicast {
            parts.push(format!("multicast {}:{}", self.config.group, self.config.port));
        }
        if self.config.broadcast {
            parts.push("broadcast".to_string());
        }
        if !self.config.seeds.is_empty() {
            parts.push(format!(
                "seeds [{}]",
                self.config
                    .seeds
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        parts.join(", ")
    }
}
