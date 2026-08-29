//! Terminal formatting helpers: tables, timestamps, colours.

use meshcore::geo::format_distance;
use meshcore::node::{Event, NetworkView, PeerView, RouteView, WhoamiView};
use meshcore::store::StoredMessage;

pub struct Style {
    enabled: bool,
}

impl Style {
    pub fn new() -> Self {
        Self {
            enabled: std::env::var_os("NO_COLOR").is_none(),
        }
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn dim(&self, t: &str) -> String {
        self.paint("2", t)
    }
    pub fn bold(&self, t: &str) -> String {
        self.paint("1", t)
    }
    pub fn cyan(&self, t: &str) -> String {
        self.paint("36", t)
    }
    pub fn green(&self, t: &str) -> String {
        self.paint("32", t)
    }
    pub fn yellow(&self, t: &str) -> String {
        self.paint("33", t)
    }
    pub fn red(&self, t: &str) -> String {
        self.paint("31", t)
    }
    pub fn magenta(&self, t: &str) -> String {
        self.paint("35", t)
    }
}

/// UTC wall clock from a millisecond timestamp - avoids pulling in a date library.
pub fn hhmmss(ts_ms: u64) -> String {
    let secs = ts_ms / 1000;
    let (h, m, s) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    format!("{h:02}:{m:02}:{s:02}")
}

pub fn ago(ms: u64) -> String {
    if ms < 1000 {
        "now".to_string()
    } else if ms < 60_000 {
        format!("{}s", ms / 1000)
    } else if ms < 3_600_000 {
        format!("{}m", ms / 60_000)
    } else {
        format!("{}h", ms / 3_600_000)
    }
}

pub fn event_line(style: &Style, event: &Event, now_ms: u64) -> String {
    match event {
        Event::Chat {
            network,
            from,
            text,
            hops,
            ..
        } => format!(
            "{} {} {}: {}",
            style.dim(&hhmmss(now_ms)),
            style.cyan(&format!("[{network}]")),
            style.bold(from),
            format!("{text} {}", style.dim(&format!("({hops}h)")))
        ),
        Event::Direct {
            network,
            from,
            text,
            hops,
            ..
        } => format!(
            "{} {} {} {}: {}",
            style.dim(&hhmmss(now_ms)),
            style.cyan(&format!("[{network}]")),
            style.magenta("(direct)"),
            style.bold(from),
            format!("{text} {}", style.dim(&format!("({hops}h)")))
        ),
        Event::PeerJoined { id, display } => style.green(&format!(
            "+ peer {display} ({}) is in range",
            short(&id.to_hex())
        )),
        Event::PeerLost { id, display } => style.dim(&format!(
            "- peer {display} ({}) went quiet",
            short(&id.to_hex())
        )),
        Event::LocationUpdate {
            display,
            gps,
            distance_m,
            ..
        } => style.green(&format!(
            "@ {display} is at {:.5}, {:.5}{}",
            gps.lat,
            gps.lon,
            distance_m
                .map(|d| format!(" ({} away)", format_distance(d)))
                .unwrap_or_default()
        )),
        Event::Delivered { to, preview } => {
            style.dim(&format!("\u{2713} delivered to {to}: \"{preview}\""))
        }
        Event::Context(name) => style.dim(&format!("context: [{name}]")),
        Event::Notice(text) => style.yellow(&format!("! {text}")),
        Event::Warning(text) => style.red(&format!("! {text}")),
    }
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

pub fn peers_table(style: &Style, peers: &[PeerView], now_ms: u64) -> String {
    if peers.is_empty() {
        return style.dim("no peers heard yet - are the other machines on the same Wi-Fi and running meshnet?").to_string();
    }
    let mut out = String::new();
    out.push_str(&style.bold(&format!(
        "{:<18} {:<16} {:<7} {:<6} {:<8} {:<10} {:<7} {}\n",
        "ID", "NAME", "LINK", "HOPS", "RTT", "DISTANCE", "SEEN", "NET"
    )));
    for p in peers {
        out.push_str(&format!(
            "{:<18} {:<16} {:<7} {:<6} {:<8} {:<10} {:<7} {}\n",
            p.id.to_hex(),
            truncate(&p.display, 16),
            if p.direct { "direct" } else { "relayed" },
            p.hops.map(|h| h.to_string()).unwrap_or_else(|| "-".into()),
            p.rtt_ms
                .map(|r| format!("{r}ms"))
                .or_else(|| p.rssi.map(|r| format!("{r}dBm")))
                .unwrap_or_else(|| "-".into()),
            p.distance_m
                .map(format_distance)
                .unwrap_or_else(|| "-".into()),
            ago(now_ms.saturating_sub(p.last_seen_ms)),
            if p.in_current_network { "yes" } else { "-" }
        ));
    }
    out.push_str(&style.dim("sorted nearest first (GPS distance, then hops, then latency)"));
    out
}

pub fn networks_table(style: &Style, nets: &[NetworkView]) -> String {
    let mut out = String::new();
    out.push_str(&style.bold(&format!(
        "{:<3} {:<18} {:<18} {:<8} {:<7} {}\n",
        "", "NAME", "ID", "MEMBERS", "EPOCH", "STORING"
    )));
    for n in nets {
        out.push_str(&format!(
            "{:<3} {:<18} {:<18} {:<8} {:<7} {}\n",
            if n.active { "*" } else { "" },
            truncate(&n.name, 18),
            if n.is_default {
                "-".to_string()
            } else {
                n.id.to_hex()
            },
            n.member_count,
            n.epoch,
            if n.store_messages { "on" } else { "off" }
        ));
        if !n.is_default && !n.members.is_empty() {
            out.push_str(&style.dim(&format!("      members: {}\n", n.members.join(", "))));
        }
    }
    out.push_str(&style.dim("* = active network. [default] membership is everyone in range."));
    out
}

pub fn routes_table(style: &Style, routes: &[RouteView]) -> String {
    if routes.is_empty() {
        return style.dim("no routes learned yet").to_string();
    }
    let mut out = String::new();
    out.push_str(&style.bold(&format!(
        "{:<18} {:<16} {:<18} {:<6} {}\n",
        "DEST", "NAME", "NEXT HOP", "HOPS", "AGE"
    )));
    for r in routes {
        out.push_str(&format!(
            "{:<18} {:<16} {:<18} {:<6} {}\n",
            r.dest.to_hex(),
            truncate(&r.display, 16),
            r.next_hop.to_hex(),
            r.hops,
            ago(r.age_ms)
        ));
    }
    out
}

pub fn history_lines(style: &Style, msgs: &[StoredMessage]) -> String {
    if msgs.is_empty() {
        return style
            .dim("no stored messages for this network (enable with --network [name] --enable-storing)")
            .to_string();
    }
    msgs.iter()
        .map(|m| {
            format!(
                "{} {} {} {}",
                style.dim(&hhmmss(m.ts_ms)),
                style.cyan(&format!("[{}]", m.network_name)),
                style.bold(&format!(
                    "{}{}",
                    short(&m.from),
                    m.to.as_deref()
                        .map(|t| format!("->{}", short(t)))
                        .unwrap_or_default()
                )),
                m.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn whoami(style: &Style, w: &WhoamiView) -> String {
    let mut out = String::new();
    out.push_str(&format!("{} {}\n", style.bold("node id  :"), w.id.to_hex()));
    out.push_str(&format!(
        "{} {}\n",
        style.bold("name     :"),
        w.name.clone().unwrap_or_else(|| "(not set)".into())
    ));
    out.push_str(&format!("{} [{}]\n", style.bold("network  :"), w.network));
    out.push_str(&format!("{} {}\n", style.bold("transport:"), w.transport));
    out.push_str(&format!("{} {}\n", style.bold("home     :"), w.home));
    out.push_str(&format!(
        "{} {}\n",
        style.bold("location :"),
        w.location
            .map(|g| format!("{:.5}, {:.5}", g.lat, g.lon))
            .unwrap_or_else(|| "(not set - use --set-location)".into())
    ));
    if !w.link_filter.is_empty() {
        out.push_str(&format!(
            "{} {}\n",
            style.bold("range    :"),
            w.link_filter
                .iter()
                .map(|i| i.to_hex())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    out.trim_end().to_string()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('~');
        t
    }
}

pub const HELP: &str = r#"commands (anything that does not start with -- is broadcast to the active network)

  messaging
    --broadcast [message]              send to everyone in the active network
    --msg [user] [message]             send a private message, routed through relays
    --history [n]                      show stored messages for the active network

  networks
    --create-network [name]            create a private network and switch to it
    --network [name] --add [user]      invite a user (seals the network key to them)
    --network [name] --enable-storing  write this network's messages to disk
    --network [name] --disable-storing stop writing them
    --switch [name]                    change the active network ([default] is public)
    --kick [user]                      vote to remove a user (>=50% of members re-keys)
    --networks                         list networks you belong to

  people and position
    --peers                            peers in range or reachable, nearest first
    --routes                           learned mesh routes and next hops
    --rename [id] [name]               local-only alias for a node id
    --set-location [lat] [lon]         set your GPS position
    --share-location                   push your position to the active network

  session
    --whoami                           your id, network, transport and home directory
    --isolate [id ...]                 pretend only these nodes are in radio range
                                       (no arguments clears it) - use it to force
                                       multi-hop relaying while testing on one LAN
    --help                             this list
    --quit                             leave

  users can be given as a node id, an id prefix, or a name set with --rename"#;
