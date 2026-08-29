# ReUnite — Offline P2P Emergency Mesh

A communication tool for when the power, the cell towers and the internet are gone. Every
machine that runs it becomes a node: it discovers the machines around it, relays traffic
for the ones it can hear, and lets people send messages, private-group messages and GPS
positions across the resulting mesh. No accounts, no sign-up, no server — the app
generates an identity on first launch and joins the mesh immediately.

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

## Where the project is

The full plan is [`plan.md`](plan.md); it is broken into executable phases in
[`phase/`](phase/README.md), one file per phase, each with its own acceptance criteria.

| Phase | Scope | Status |
| :--- | :--- | :--- |
| [1](phase/phase-1-terminal-mvp.md) | Rust core + terminal MVP | **in progress** — see the step table below |
| [2](phase/phase-2-mobile.md) | iOS/Android app on the real core | not started; `mobile/` is a UI shell over mock data |
| [3](phase/phase-3-embedded.md) | `no_std` core, ESP32/nRF52, drone relays | not started |

### Phase 1, step by step

Numbering follows the current `plan.md` §4.

| Step | Status |
| :--- | :--- |
| 1.1 Hashed node id, zero-config `[default]` onboarding, GPS beacons | done |
| 1.1 Connectionless discovery | done over UDP multicast/broadcast; BLE on Linux only |
| 1.1 Battery-level telemetry | **not started** |
| 1.2 Store-and-forward routing with TTL, RSSI/latency route table | done (RSSI awaits a radio that reports it) |
| 1.2 `clap` CLI, local state, `--rename` aliases | done |
| 1.2 Binary-packed pre-canned messages | **not started** |
| 1.3 Private networks, X25519 sealed key exchange, `--enable-storing` | done |
| 1.3 Decentralised kick voting with automatic re-key | done |
| 1.4 In-network SOS | **not started** |
| 1.4 "Last known location" ghosting | partial — last GPS and timestamp are cached, nothing renders them |
| 1.5 H3 hex-grid heat map with trust consensus | **not started** |

**One honest deviation.** Phase 1 runs the mesh over **Wi-Fi (UDP), not BLE**. Laptops
cannot portably *advertise* as BLE peripherals — `btleplug` and the equivalent libraries
are central/scanner-only on macOS and Windows — so a laptop-to-laptop BLE mesh is not
buildable today, while a Wi-Fi one is, and needs no internet or router uplink. The radio
lives behind the [`Transport`](crates/meshcore/src/transport/mod.rs) trait, so the BLE and
Wi-Fi Direct adapters of Phase 2 slot in underneath the same routing, crypto and CLI code.
Every other gap between `plan.md` and the code is tracked in the
[deviations register](phase/README.md#deviations-register).

## Quick start

```bash
# 1. Install Rust (once, per machine) — https://rustup.rs
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # macOS / Linux

# 2. Build
git clone <this-repo> && cd ReUnite
cargo build --release

# 3. Run — on every laptop, joined to the same Wi-Fi
./target/release/meshnet --name alice
```

Two laptops on the same Wi-Fi find each other in a few seconds, with no configuration and
no internet. **The full cross-machine guide, including how to make a Wi-Fi network when
there is no router, firewall rules and troubleshooting, is
[docs/SETUP.md](docs/SETUP.md).**

### Bluetooth

* **Linux:** `meshnet --transport ble` talks to BlueZ directly and advertises as a
  peripheral on service UUID `a1b2c3d4-e5f6-7890-1234-56789abcdef0`.
* **macOS / Windows:** run `python3 scripts/ble_gateway.py`, which bridges the BLE radio
  to a local `meshnet` node over UDP.

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

### Planned in Phase 1 — not implemented yet

These are specified in [`plan.md`](plan.md) §5 and
[`phase/phase-1-terminal-mvp.md`](phase/phase-1-terminal-mvp.md). Typing them today returns
`unknown command`.

| Command | Will do |
| :--- | :--- |
| `--sos start` / `--sos stop` | Toggle the high-priority in-network SOS broadcast |
| `--status [code]` | Send a pre-canned 1-byte message (`1` = safe, `2` = medical, …) |
| `--report-zone [lat] [lon] [lvl]` | Submit a safety report, aggregated into an H3 hex grid |
| `--heatmap show` | Dump the aggregated safety zones with their trust-consensus counts |
| `--battery [pct]` | Override the reported battery level (for demos and tests) |

> **The in-network SOS is deliberately isolated from your phone's or laptop's emergency
> services SOS.** It raises an alert on the local mesh only. It does not, and will not,
> call emergency services.

## Mobile

`mobile/` is a Flutter app: three tabs (Chat, GPS & Peers, Networks), dark theme, real GPS
via `geolocator`, and Android/iOS Bluetooth and location permissions already declared.

**It is not yet connected to the mesh.** `lib/services/mesh_service.dart` is mock data — a
hard-coded node id and one fake peer — and no Rust bridge exists. Wiring it to `meshcore`
through `flutter_rust_bridge`, with native Kotlin and Swift BLE underneath, is
[Phase 2](phase/phase-2-mobile.md).

## Documentation

* [phase/](phase/README.md) — the phase plan, acceptance criteria and deviations register
* [docs/SETUP.md](docs/SETUP.md) — install, build and run across several computers, step by step
* [docs/DEMO.md](docs/DEMO.md) — a scripted three-laptop demo, including multi-hop relaying
* [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — protocol, routing, security model and limits
* [plan.md](plan.md) — the project plan; [proposal.md](proposal.md) — the original scenario

## Layout

```
crates/meshcore   the mesh: identity, crypto, packets, routing, transports, node actor
crates/meshcli    the terminal client (a thin shell over meshcore)
mobile/           Flutter app (UI shell; not yet on the real core)
scripts/          ble_gateway.py — BLE <-> UDP bridge for macOS and Windows
phase/            phase-by-phase build plan
```

## Tests

```bash
cargo test
```

Covers the sealed-key exchange, packet signing and tamper rejection, duplicate-flood
suppression, route preference, simulated radio range, kick thresholds and re-keying, state
persistence and GPS distance.
