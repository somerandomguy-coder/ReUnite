//! `meshnet` - terminal client for the offline P2P emergency mesh (plan.md Phase 1).
//!
//! Start it, land in `[default]`, see who is in range, talk. Everything below is a thin
//! shell: parsing a typed line into a `meshcore::Command` and printing the `Event`s that
//! come back.

mod render;

use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use meshcore::node::{Command, Event, Node, NodeConfig, Reply};
use meshcore::store::resolve_home;
use meshcore::transport::{UdpConfig, UdpTransport};
use meshcore::types::{now_ms, Gps, NodeId};
use tokio::sync::mpsc;

use render::Style;

#[derive(Parser, Debug)]
#[command(
    name = "meshnet",
    about = "Offline peer-to-peer emergency mesh network (terminal client)",
    version
)]
struct Args {
    /// State directory (identity, contacts, networks, stored messages).
    #[arg(long)]
    home: Option<PathBuf>,

    /// Name broadcast to peers. They can still override it locally with --rename.
    #[arg(long)]
    name: Option<String>,

    /// UDP port to bind. Every node on a LAN should use the same one.
    #[arg(long, default_value_t = 47474)]
    port: u16,

    /// IPv4 multicast group used for discovery.
    #[arg(long, default_value = "239.42.13.7")]
    group: Ipv4Addr,

    /// Extra peer to contact directly, e.g. 192.168.1.42:47474. Repeatable.
    /// Needed when multicast is blocked, or when running several nodes on one machine.
    #[arg(long = "peer", value_name = "HOST:PORT")]
    peers: Vec<String>,

    /// Disable multicast discovery.
    #[arg(long)]
    no_multicast: bool,

    /// Disable subnet broadcast discovery.
    #[arg(long)]
    no_broadcast: bool,

    /// Starting latitude.
    #[arg(long, requires = "lon")]
    lat: Option<f64>,

    /// Starting longitude.
    #[arg(long, requires = "lat")]
    lon: Option<f64>,

    /// Transport layer to use: udp (Wi-Fi/LAN) or ble (Bluetooth Low Energy).
    #[arg(long, default_value = "udp")]
    transport: String,

    /// Only hear these node ids (simulated radio range, for testing multi-hop routing).
    #[arg(long = "isolate", value_name = "NODE_ID", num_args = 1..)]
    isolate: Vec<String>,

    /// Report this battery percentage instead of asking the platform. Keeps demos
    /// deterministic and gives a mains-powered desktop something to advertise.
    #[arg(long, value_name = "PCT", value_parser = clap::value_parser!(u8).range(0..=100))]
    battery: Option<u8>,

    /// H3 resolution for safe-zone aggregation (higher = smaller cells).
    #[arg(long, default_value_t = meshcore::zones::DEFAULT_RESOLUTION)]
    zone_resolution: u8,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let style = Style::new();

    let home = resolve_home(args.home.clone())?;
    use meshcore::transport::Transport;

    let (transport, transport_description): (Arc<dyn Transport>, String) = match args.transport.to_lowercase().as_str() {
        "ble" => {
            #[cfg(target_os = "linux")]
            {
                let ble = meshcore::transport::BleLinuxTransport::bind(args.name.clone()).await?;
                let desc = ble.describe();
                (Arc::new(ble), desc)
            }
            #[cfg(not(target_os = "linux"))]
            {
                bail!("Native Rust BLE transport is available directly on Linux via BlueZ. On macOS, run `python3 scripts/ble_gateway.py` to bridge BLE radio traffic to meshnet UDP!");
            }
        }
        _ => {
            let seeds = resolve_seeds(&args.peers)?;
            let udp = UdpTransport::bind(UdpConfig {
                port: args.port,
                group: args.group,
                multicast: !args.no_multicast,
                broadcast: !args.no_broadcast,
                seeds,
            })
            .with_context(|| {
                format!(
                    "could not bind UDP port {} - is another node already using it? try --port {}",
                    args.port,
                    args.port + 1
                )
            })?;
            let desc = udp.describe();
            (Arc::new(udp), desc)
        }
    };

    let mut config = NodeConfig::new(home.clone());
    config.self_name = args.name.clone();
    config.location = match (args.lat, args.lon) {
        (Some(lat), Some(lon)) => Some(Gps {
            lat,
            lon,
            ts_ms: now_ms(),
        }),
        _ => None,
    };
    config.battery_override = args.battery;
    config.zone_resolution = args.zone_resolution;
    config.link_filter = args
        .isolate
        .iter()
        .map(|s| NodeId::from_hex(s).with_context(|| format!("--isolate {s}")))
        .collect::<Result<HashSet<_>>>()?;

    let (handle, mut events) = Node::spawn(config, transport)?;


    println!(
        "{}",
        style.bold("offline mesh node started - you are in [default]")
    );
    println!("  node id  : {}", style.cyan(&handle.id.to_hex()));
    println!("  transport: {transport_description}");
    println!("  home     : {}", home.display());
    println!(
        "  {}",
        style.dim("type --help for commands, or just type a message to broadcast")
    );

    let mut prompt_network = "default".to_string();
    let mut lines = stdin_lines();
    print_prompt(&style, &prompt_network);

    loop {
        tokio::select! {
            line = lines.recv() => {
                let Some(line) = line else { break };
                match handle_line(&handle, &style, &line).await {
                    Ok(Outcome::Continue) => {}
                    Ok(Outcome::Quit) => break,
                    Err(e) => println!("{}", style.red(&format!("! {e}"))),
                }
                print_prompt(&style, &prompt_network);
            }
            event = events.recv() => {
                let Some(event) = event else { break };
                if let Event::Context(name) = &event {
                    prompt_network = name.clone();
                }
                clear_line();
                println!("{}", render::event_line(&style, &event, now_ms()));
                print_prompt(&style, &prompt_network);
            }
        }
    }

    println!("\n{}", style.dim("mesh node stopped"));
    Ok(())
}

enum Outcome {
    Continue,
    Quit,
}

fn print_prompt(style: &Style, network: &str) {
    print!("{} ", style.cyan(&format!("[{network}] >")));
    let _ = std::io::stdout().flush();
}

fn clear_line() {
    print!("\r\x1b[2K");
    let _ = std::io::stdout().flush();
}

/// Read stdin on a blocking thread so the async event loop never stalls on the keyboard.
fn stdin_lines() -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel(16);
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(line) => {
                    if tx.blocking_send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    rx
}

fn resolve_seeds(peers: &[String]) -> Result<Vec<SocketAddr>> {
    let mut out = Vec::new();
    for peer in peers {
        let mut addrs = peer
            .to_socket_addrs()
            .with_context(|| format!("--peer {peer}: expected HOST:PORT"))?;
        let addr = addrs
            .find(|a| a.is_ipv4())
            .ok_or_else(|| anyhow!("--peer {peer}: no IPv4 address"))?;
        out.push(addr);
    }
    Ok(out)
}

async fn handle_line(
    handle: &meshcore::node::NodeHandle,
    style: &Style,
    line: &str,
) -> Result<Outcome> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(Outcome::Continue);
    }
    // Anything not starting with `--` is a shout to the active network.
    if !trimmed.starts_with("--") {
        let reply = handle.call(Command::Broadcast(trimmed.to_string())).await?;
        show(style, reply);
        return Ok(Outcome::Continue);
    }

    let tokens = tokenize(trimmed);
    let cmd = tokens[0].as_str();
    let rest = &tokens[1..];

    match cmd {
        "--help" | "-h" => {
            println!("{}", render::HELP);
            Ok(Outcome::Continue)
        }
        "--quit" | "--exit" => Ok(Outcome::Quit),
        "--status" if rest.is_empty() => {
            println!("{}", render::status_table(style));
            Ok(Outcome::Continue)
        }
        _ => {
            let command = parse_command(cmd, rest)?;
            let reply = handle.call(command).await?;
            show(style, reply);
            Ok(Outcome::Continue)
        }
    }
}

fn parse_command(cmd: &str, rest: &[String]) -> Result<Command> {
    match cmd {
        "--broadcast" => {
            let text = rest.join(" ");
            if text.trim().is_empty() {
                bail!("usage: --broadcast [message]");
            }
            Ok(Command::Broadcast(text))
        }
        "--msg" | "--dm" => {
            if rest.len() < 2 {
                bail!("usage: --msg [user] [message]");
            }
            Ok(Command::Direct {
                target: rest[0].clone(),
                text: rest[1..].join(" "),
            })
        }
        "--create-network" => {
            let name = rest.first().ok_or_else(|| anyhow!("usage: --create-network [name]"))?;
            Ok(Command::CreateNetwork(name.clone()))
        }
        // --network [name] --add [user] | --enable-storing | --disable-storing
        "--network" => {
            let network = rest
                .first()
                .ok_or_else(|| anyhow!("usage: --network [name] --add [user] | --enable-storing | --disable-storing"))?
                .clone();
            match rest.get(1).map(String::as_str) {
                Some("--add") => {
                    let user = rest
                        .get(2)
                        .ok_or_else(|| anyhow!("usage: --network [name] --add [user]"))?;
                    Ok(Command::Invite {
                        network,
                        user: user.clone(),
                    })
                }
                Some("--enable-storing") => Ok(Command::SetStoring { network, on: true }),
                Some("--disable-storing") => Ok(Command::SetStoring { network, on: false }),
                Some(other) => bail!("unknown option '{other}' after --network [name]"),
                None => bail!("usage: --network [name] --add [user] | --enable-storing | --disable-storing"),
            }
        }
        "--kick" => {
            let user = rest.first().ok_or_else(|| anyhow!("usage: --kick [user]"))?;
            Ok(Command::Kick(user.clone()))
        }
        "--rename" => {
            if rest.len() < 2 {
                bail!("usage: --rename [id] [name]");
            }
            Ok(Command::Rename {
                user: rest[0].clone(),
                name: rest[1..].join(" "),
            })
        }
        "--switch" => {
            let name = rest.first().ok_or_else(|| anyhow!("usage: --switch [network]"))?;
            Ok(Command::Switch(name.clone()))
        }
        "--peers" => Ok(Command::Peers),
        "--networks" => Ok(Command::Networks),
        "--routes" => Ok(Command::Routes),
        "--whoami" => Ok(Command::Whoami),
        "--history" => {
            let limit = rest
                .first()
                .map(|n| n.parse::<usize>())
                .transpose()
                .map_err(|_| anyhow!("usage: --history [count]"))?
                .unwrap_or(30);
            Ok(Command::History(limit))
        }
        "--set-location" => {
            if rest.len() < 2 {
                bail!("usage: --set-location [lat] [lon]");
            }
            let lat: f64 = rest[0].parse().map_err(|_| anyhow!("bad latitude"))?;
            let lon: f64 = rest[1].parse().map_err(|_| anyhow!("bad longitude"))?;
            Ok(Command::SetLocation { lat, lon })
        }
        "--share-location" => Ok(Command::ShareLocation),
        "--sos" => match rest.first().map(String::as_str) {
            Some("start") | Some("on") => Ok(Command::Sos(true)),
            Some("stop") | Some("off") => Ok(Command::Sos(false)),
            _ => bail!("usage: --sos start | --sos stop (in-network only, never emergency services)"),
        },
        "--status" => {
            let arg = rest
                .first()
                .ok_or_else(|| anyhow!("usage: --status [code|name]"))?;
            let code = meshcore::status::parse(arg)
                .ok_or_else(|| anyhow!("unknown status '{arg}' - run --status with no argument for the list"))?;
            Ok(Command::SetStatus { code })
        }
        "--report-zone" => {
            if rest.len() < 3 {
                bail!("usage: --report-zone [lat] [lon] [level 0-4]");
            }
            let lat: f64 = rest[0].parse().map_err(|_| anyhow!("bad latitude"))?;
            let lon: f64 = rest[1].parse().map_err(|_| anyhow!("bad longitude"))?;
            let level: u8 = rest[2]
                .parse()
                .map_err(|_| anyhow!("bad level - use 0 (dangerous) to 4 (safe)"))?;
            Ok(Command::ReportZone { lat, lon, level })
        }
        "--heatmap" => match rest.first().map(String::as_str) {
            None | Some("show") => Ok(Command::Heatmap),
            Some(other) => bail!("unknown option '{other}' - usage: --heatmap show"),
        },
        "--isolate" => Ok(Command::SetLinkFilter(rest.to_vec())),
        other => Err(anyhow!("unknown command '{other}' - try --help")),
    }
}

fn show(style: &Style, reply: Reply) {
    match reply {
        Reply::Ok(text) => println!("{}", style.green(&text)),
        Reply::Peers(peers) => println!("{}", render::peers_table(style, &peers, now_ms())),
        Reply::Networks(nets) => println!("{}", render::networks_table(style, &nets)),
        Reply::Routes(routes) => println!("{}", render::routes_table(style, &routes)),
        Reply::History(msgs) => println!("{}", render::history_lines(style, &msgs)),
        Reply::Whoami(w) => println!("{}", render::whoami(style, &w)),
        Reply::Heatmap(zones) => println!("{}", render::heatmap_table(style, &zones)),
    }
}

/// Split a line into tokens, honouring double quotes so names can contain spaces.
fn tokenize(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in line.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}
