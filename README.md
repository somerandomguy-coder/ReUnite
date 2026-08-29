# Offline P2P Emergency Mesh — Terminal MVP

A laptop-to-laptop emergency communication tool for when the power, the cell towers and
the internet are gone. Every machine that runs it becomes a node: it discovers the
machines around it, relays traffic for the ones it can hear, and lets people send
messages, private-group messages and GPS positions across the resulting mesh.

This repository implements **Phase 1 of [`plan.md`](plan.md)** — the terminal MVP.

```
$ meshnet
offline mesh node started - you are in [default]
  node id  : b809ed72839ec44b
  transport: udp/0.0.0.0:47474, multicast 239.42.13.7:47474, broadcast
  home     : /Users/you/.meshnet
[default] > this is alice, we need water at block 4
[default] > --peers
ID                 NAME             LINK    HOPS   RTT      DISTANCE   SEEN    NET
d94753fd0112feca   ~bob             direct  1      14ms     345m       1s      yes
16ff2f7c934f4ac9   ~carol           relayed 2      -        1.46km     1s      yes
sorted nearest first (GPS distance, then hops, then latency)
```

## What works today

| plan.md Phase 1 step | Status |
| :--- | :--- |
| 1.1 Node identity, discovery, `[default]` network, GPS beacons | done |
| 1.2 CLI, local state, `--rename` aliases | done |
| 1.3 Private networks, sealed key exchange, `--enable-storing` | done |
| 1.4 Decentralised kick voting with automatic re-key | done |
| 1.5 Store-and-forward relaying, RSSI/latency-aware route table | done (RSSI field awaits a radio that reports it) |

**One honest deviation from the plan.** Phase 1 runs the mesh over **Wi-Fi (UDP), not
BLE**. Laptops cannot portably *advertise* as BLE peripherals — `btleplug` and the
equivalent libraries are central/scanner-only on macOS and Windows — so a laptop-to-laptop
BLE mesh is not buildable today, while a Wi-Fi one is, and needs no internet or router
uplink. The radio lives behind the [`Transport`](crates/meshcore/src/transport/mod.rs)
trait, so the BLE and Wi-Fi Direct adapters of Phase 2 slot in underneath the same
routing, crypto and CLI code. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Quick start

```bash
# 1. Install Rust (once, per machine) — https://rustup.rs
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # macOS / Linux

# 2. Build
git clone <this-repo> && cd UniHack2026
cargo build --release

# 3. Run — on every laptop, joined to the same Wi-Fi
./target/release/meshnet --name alice
```

Two laptops on the same Wi-Fi find each other in a few seconds, with no configuration and
no internet. **The full cross-machine guide, including how to make a Wi-Fi network when
there is no router, firewall rules and troubleshooting, is
[docs/SETUP.md](docs/SETUP.md).**

## Commands

Type them at the `[network] >` prompt. Anything that does not start with `--` is
broadcast to the network you are currently in.

| Command | What it does |
| :--- | :--- |
| `--broadcast [message]` | Send to everyone in the active network |
| `--msg [user] [message]` | Private message, routed (and relayed) to one node |
| `--create-network [name]` | Create a private network and switch to it |
| `--network [name] --add [user]` | Invite a user; seals the network key to their public key |
| `--network [name] --enable-storing` | Write this network's messages to disk |
| `--network [name] --disable-storing` | Stop writing them |
| `--switch [name]` | Change the active network |
| `--kick [user]` | Vote to remove a user; at >=50% the network re-keys without them |
| `--rename [id] [name]` | Local-only alias for a node id |
| `--peers` | Who is reachable, nearest first |
| `--routes` | Learned routes and next hops |
| `--networks` | Networks you belong to |
| `--history [n]` | Stored messages for the active network |
| `--set-location [lat] [lon]` / `--share-location` | Set and publish your GPS position |
| `--whoami` | Your id, network, transport, home directory |
| `--isolate [id ...]` | Pretend only these nodes are in radio range (testing) |
| `--help` / `--quit` | |

A `[user]` may be a full node id, a unique id prefix, or a name you set with `--rename`.

## Documentation

* [docs/SETUP.md](docs/SETUP.md) — install, build and run across several computers, step by step
* [docs/DEMO.md](docs/DEMO.md) — a scripted three-laptop demo, including multi-hop relaying
* [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — protocol, routing, security model and limits
* [plan.md](plan.md) — the original project plan; [proposal.md](proposal.md) — the scenario

## Layout

```
crates/meshcore   the mesh: identity, crypto, packets, routing, transports, node actor
crates/meshcli    the terminal client (a thin shell over meshcore)
```

## Tests

```bash
cargo test
```

Covers the sealed-key exchange, packet signing and tamper rejection, duplicate-flood
suppression, route preference, kick thresholds and re-keying, and state persistence.
