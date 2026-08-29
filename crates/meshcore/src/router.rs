//! Neighbour table, route table and flood control - plan.md §3.1 and §4 step 1.5.
//!
//! Routing is "path-recorded flooding with learned reverse routes":
//!   * every packet carries a TTL and the list of relays that already touched it;
//!   * a duplicate packet id is dropped, which is what keeps a dense room from melting
//!     down (plan.md §6.2 broadcast storms);
//!   * receiving a packet teaches us a route back to its origin - next hop is the
//!     neighbour that handed it to us, cost is hop count plus measured round-trip time;
//!   * a packet addressed to a node we have a route for is unicast to that next hop
//!     instead of being flooded, and falls back to flooding when the route is unknown.

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddr;

use crate::types::{now_ms, NodeId};

/// A node we can hear directly (one radio hop / one datagram away).
#[derive(Clone, Debug)]
pub struct Neighbor {
    pub id: NodeId,
    pub addr: SocketAddr,
    pub first_seen_ms: u64,
    pub last_seen_ms: u64,
    /// Round-trip time from the last ping/pong, our stand-in for link quality.
    pub rtt_ms: Option<u64>,
    /// Radio signal strength. Always `None` on the UDP transport; the BLE adapter
    /// fills this in (plan.md §3.1 RSSI-based route selection).
    pub rssi: Option<i16>,
}

#[derive(Clone, Debug)]
pub struct Route {
    pub dest: NodeId,
    pub next_hop: NodeId,
    pub hops: u8,
    pub last_seen_ms: u64,
}

pub struct Router {
    me: NodeId,
    neighbors: HashMap<NodeId, Neighbor>,
    routes: HashMap<NodeId, Route>,
    seen: HashSet<crate::types::MsgId>,
    seen_order: VecDeque<(crate::types::MsgId, u64)>,
    /// Simulated radio range: when non-empty, frames from any other node are dropped.
    link_filter: HashSet<NodeId>,
    pub neighbor_timeout_ms: u64,
    pub route_timeout_ms: u64,
    seen_capacity: usize,
}

impl Router {
    pub fn new(me: NodeId) -> Self {
        Self {
            me,
            neighbors: HashMap::new(),
            routes: HashMap::new(),
            seen: HashSet::new(),
            seen_order: VecDeque::new(),
            link_filter: HashSet::new(),
            neighbor_timeout_ms: 30_000,
            route_timeout_ms: 120_000,
            seen_capacity: 4096,
        }
    }

    pub fn set_link_filter(&mut self, allowed: HashSet<NodeId>) {
        self.link_filter = allowed;
    }

    pub fn link_filter(&self) -> &HashSet<NodeId> {
        &self.link_filter
    }

    /// Link-layer admission control, used to fake radio range during testing.
    pub fn accepts_link(&self, from: &NodeId) -> bool {
        self.link_filter.is_empty() || self.link_filter.contains(from)
    }

    pub fn note_neighbor(&mut self, id: NodeId, addr: SocketAddr) -> bool {
        let now = now_ms();
        match self.neighbors.get_mut(&id) {
            Some(n) => {
                n.addr = addr;
                n.last_seen_ms = now;
                false
            }
            None => {
                self.neighbors.insert(
                    id,
                    Neighbor {
                        id,
                        addr,
                        first_seen_ms: now,
                        last_seen_ms: now,
                        rtt_ms: None,
                        rssi: None,
                    },
                );
                // A direct neighbour is always the best possible route to itself.
                self.routes.insert(
                    id,
                    Route {
                        dest: id,
                        next_hop: id,
                        hops: 1,
                        last_seen_ms: now,
                    },
                );
                true
            }
        }
    }

    pub fn note_rtt(&mut self, id: &NodeId, rtt_ms: u64) {
        if let Some(n) = self.neighbors.get_mut(id) {
            n.rtt_ms = Some(rtt_ms);
        }
    }

    pub fn note_rssi(&mut self, id: &NodeId, rssi: i16) {
        if let Some(n) = self.neighbors.get_mut(id) {
            n.rssi = Some(rssi);
        }
    }

    /// Learn (or refresh) a route to `origin` via the neighbour that relayed to us.
    pub fn learn_route(&mut self, origin: NodeId, next_hop: NodeId, hops: u8) {
        if origin == self.me {
            return;
        }
        let now = now_ms();
        let better = match self.routes.get(&origin) {
            None => true,
            Some(existing) => {
                hops < existing.hops || now.saturating_sub(existing.last_seen_ms) > 15_000
            }
        };
        if better {
            self.routes.insert(
                origin,
                Route {
                    dest: origin,
                    next_hop,
                    hops,
                    last_seen_ms: now,
                },
            );
        } else if let Some(existing) = self.routes.get_mut(&origin) {
            if existing.next_hop == next_hop && existing.hops == hops {
                existing.last_seen_ms = now;
            }
        }
    }

    /// True the first time we see a packet id; false for every duplicate.
    pub fn mark_seen(&mut self, id: crate::types::MsgId) -> bool {
        if self.seen.contains(&id) {
            return false;
        }
        self.seen.insert(id);
        self.seen_order.push_back((id, now_ms()));
        while self.seen_order.len() > self.seen_capacity {
            if let Some((old, _)) = self.seen_order.pop_front() {
                self.seen.remove(&old);
            }
        }
        true
    }

    pub fn next_hop_addr(&self, dest: &NodeId) -> Option<SocketAddr> {
        let route = self.routes.get(dest)?;
        if now_ms().saturating_sub(route.last_seen_ms) > self.route_timeout_ms {
            return None;
        }
        self.neighbors.get(&route.next_hop).map(|n| n.addr)
    }

    pub fn has_route(&self, dest: &NodeId) -> bool {
        self.next_hop_addr(dest).is_some()
    }

    pub fn route(&self, dest: &NodeId) -> Option<&Route> {
        self.routes.get(dest)
    }

    pub fn neighbor(&self, id: &NodeId) -> Option<&Neighbor> {
        self.neighbors.get(id)
    }

    pub fn neighbors(&self) -> impl Iterator<Item = &Neighbor> {
        self.neighbors.values()
    }

    pub fn routes(&self) -> impl Iterator<Item = &Route> {
        self.routes.values()
    }

    /// Drop neighbours we have not heard from in a while. Returns the ids that went away.
    pub fn prune(&mut self) -> Vec<NodeId> {
        let now = now_ms();
        let timeout = self.neighbor_timeout_ms;
        let gone: Vec<NodeId> = self
            .neighbors
            .values()
            .filter(|n| now.saturating_sub(n.last_seen_ms) > timeout)
            .map(|n| n.id)
            .collect();
        for id in &gone {
            self.neighbors.remove(id);
        }
        let route_timeout = self.route_timeout_ms;
        self.routes.retain(|_, r| {
            now.saturating_sub(r.last_seen_ms) <= route_timeout && !gone.contains(&r.next_hop)
        });
        gone
    }
}
