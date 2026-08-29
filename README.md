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
| [1](phase/phase-1-terminal-mvp.md) | Rust core + terminal MVP | **complete** — see the step table below |
| [2](phase/phase-2-mobile.md) | iOS/Android app on the real core | **in progress** — the app runs on the real core over Wi-Fi; native BLE and background execution are still to do |
| [2A](phase/phase-2a-build-and-display-integrity.md) | Build and display integrity | **complete** — green analyzer, green tests, docs that match the code |
| [2B](phase/phase-2b-safe-unsafe-zones.md) | Safe/unsafe zones over a chosen radius | **complete** — replaced the 0–4 gradient |
| [2C](phase/phase-2c-ble-interop.md) | Android ↔ iOS Bluetooth interoperability | **fix landed** — iOS never started its radio; needs a two-phone test |
| [2D](phase/phase-2d-zero-touch-join.md) | Zero-touch join, every radio at once, adaptive duty cycle | **complete** — battery drain still unmeasured |
| [2E](phase/phase-2e-hardware-verification.md) | Run 2C and 2D on two real phones | **next** — the only phase that needs hardware |
| [3](phase/phase-3-embedded.md) | `no_std` core, ESP32/nRF52, drone relays | not started |

### Phase 1, step by step

Numbering follows the current `plan.md` §4.

| Step | Status |
| :--- | :--- |
| 1.1 Hashed node id, zero-config `[default]` onboarding, GPS beacons | done |
| 1.1 Connectionless discovery | done over UDP multicast/broadcast; BLE on Linux only |
| 1.1 Battery-level telemetry | done (macOS + Linux; `--battery` overrides) |
| 1.1 Beacon v1, the 27-byte BLE-advertisement wire format | done (encoded and tested; needs a Phase 2 radio to emit it) |
| 1.2 Store-and-forward routing with TTL, RSSI/latency route table | done — RSSI wired to the Bluetooth scanners in phase 2C |
| 1.2 `clap` CLI, local state, `--rename` aliases | done |
| 1.2 Binary-packed pre-canned messages | done — one byte each. Shipped with 7 codes; the UI table was cut to 3 (Safe / Hazard / SOS) in Phase 2, and the other 4 remain valid wire codes an older or newer build may send |
| 1.3 Private networks, X25519 sealed key exchange, `--enable-storing` | done |
| 1.3 Decentralised kick voting with automatic re-key | done |
| 1.4 In-network SOS | done — longer TTL, isolated from OS emergency services |
| 1.4 "Last known location" ghosting | done — unreachable peers stay on `--peers` at their last fix |
| 1.5 H3 hex-grid safe zones with trust consensus | done — resolution 8, one report per node per cell. Phase 2B replaced the 0–4 scale with a safe/unsafe verdict over a radius the reporter chooses |

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

`meshnet` starts **every radio the machine has** and meshes over all of them at once. A
radio that cannot start is named and skipped — a laptop with no Bluetooth still meshes
over Wi-Fi:

```
$ meshnet
  radios   : udp/0.0.0.0:47474, multicast 239.42.13.7:47474, broadcast
  not used - bluetooth: this OS cannot advertise as a BLE peripheral - run scripts/ble_gateway.py to bridge one
```

`--transport udp` or `--transport ble` pins one, for testing.

### Join on boot

```bash
./scripts/autostart/install.sh            # launchd on macOS, systemd --user on Linux
./scripts/autostart/install.sh --remove
```

### Bluetooth

* **Linux:** BlueZ directly, advertising as a peripheral on service UUID
  `a1b2c3d4-e5f6-7890-1234-56789abcdef0`. Started automatically alongside Wi-Fi.
* **macOS / Windows:** no portable BLE peripheral role exists, so these mesh over Wi-Fi.
  Run `python3 scripts/ble_gateway.py` to bridge a BLE radio to a local node over UDP.

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
| `--sos start` / `--sos stop` | Raise or clear the in-network SOS |
| `--status [code\|name]` | Send a pre-canned 1-byte message; no argument lists them |
| `--report-zone [lat] [lon] safe\|unsafe [radius] [unit]` | Is the area around a point safe? Unit is `m`, `km`, `ft` or `mi`; snapped to an H3 cell before it is sent |
| `--heatmap show` | Reported zones, unsafe first, with the vote count on each side |
| `--whoami` | Your id, network, transport, home directory |
| `--isolate [id ...]` | Pretend only these nodes are in radio range (testing) |
| `--help` / `--quit` | |

A `[user]` may be a full node id, a unique id prefix, or a name you set with `--rename`.

Startup flags: `--home`, `--name`, `--port`, `--group`, `--peer`, `--lat`/`--lon`,
`--transport`, `--isolate`, `--battery [0-100]` (override the reported charge) and
`--zone-resolution` (H3 resolution for the heat map).

> **The in-network SOS is deliberately isolated from your phone's or laptop's emergency
> services SOS.** It raises an alert on the local mesh only. It does not, and will not,
> call emergency services.

### Emergency features, in one minute

```
[default] > --status medical            # one byte on the wire, not the words
[default] > --sos start                 # ttl 12, mesh alert only
[default] > --report-zone 10.7769 106.7009 unsafe 750 m
[default] > --report-zone 10.8269 106.7509 safe 0.5 km
[default] > --heatmap show
CELL               LAT          LON          VERDICT   RADIUS    SAFE    UNSAFE  AGE    MINE
8865b5662bfffff    10.77508     106.69941    unsafe    750 m     0       1       now    yes
8865b56767fffff    10.82372     106.75430    safe      500 m     1       0       now    yes

[default] > --peers
ID                 NAME             LINK     HOPS   RTT      DISTANCE   BATT   SEEN    NET
acfd53bb3f4e5430   ~carol           relayed  2      -        -          7%     now     yes
  !! SOS - last heard just now
d965fdd41a1a1940   ~doomed          ghost    -      -        1.11km     3%     38s     yes
  last seen at 10.78690, 106.70090 38s ago
  * I am safe
```

A peer whose battery dies becomes a dimmed **ghost** at their last known position rather
than vanishing.

A zone report answers one question — **is it safe here, yes or no** — about a radius the
reporter chooses, because the person on the ground knows whether they can vouch for their
doorway or for the whole district. `SAFE` and `UNSAFE` are the number of distinct people
on each side, deliberately shown as two numbers rather than one: one person calling a
street safe is not the same claim as thirty, and five people disagreeing with four is not
the same claim as five agreeing. **A tie reads unsafe** — a contested area is not a safe
area. The app draws each zone as a translucent circle that darkens where reports overlap.

## Mobile and desktop app

`mobile/` is a Flutter app running **the same mesh core as the CLI** — routing, crypto,
SOS, panic codes, ghosting and zone consensus all happen in Rust, reached over a small
C ABI (`crates/meshffi`) through `dart:ffi`.

```bash
./scripts/build_ffi.sh macos      # or: android | ios
cd mobile && flutter run -d macos # or a phone from `flutter devices`
```

Four screens: **Chat**, **Peers** (compass/grid radar with bearing and distance, ghosts,
battery, SOS), **Emergency** (slide-to-activate SOS, one-tap panic buttons read from the
Rust status table, zone
reporting, heat map with consensus) and **Networks** (create, invite, switch, storing,
kick).

**Full setup and a step-by-step test script: [docs/MOBILE.md](docs/MOBILE.md).**

The app starts **every radio the phone has** — Bluetooth and Wi-Fi together — with no
picker and nothing to configure. It slows its beacon and scan down when nobody is around
and snaps back the moment anything is heard, never while an SOS is active.

One honest limit: it does not run in the background yet, so the app has to stay on screen.
That is [Phase 2](phase/phase-2-mobile.md) step 2.5.

## Documentation

* [docs/JOINING.md](docs/JOINING.md) — **for users**: switch the device on, and what to do when nothing appears
* [phase/](phase/README.md) — the phase plan, acceptance criteria and deviations register
* [docs/MOBILE.md](docs/MOBILE.md) — run the app on a laptop and on phones, and test every feature
* [docs/SETUP.md](docs/SETUP.md) — install, build and run the CLI across several computers
* [docs/DEMO.md](docs/DEMO.md) — a scripted three-laptop demo, including multi-hop relaying
* [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — protocol, routing, security model and limits
* [plan.md](plan.md) — the project plan; [proposal.md](proposal.md) — the original scenario

## Layout

```
crates/meshcore   the mesh: identity, crypto, packets, routing, transports, node actor
crates/meshcli    the terminal client (a thin shell over meshcore)
crates/meshffi    C ABI bridge so the Flutter app runs the same core
mobile/           Flutter app for macOS, Android and iOS
scripts/          build_ffi.sh — build the core per platform
                  ble_gateway.py — BLE <-> UDP bridge for macOS and Windows
phase/            phase-by-phase build plan
```

## Tests

```bash
cargo test          # 34 Rust tests
./scripts/check.sh  # ...plus flutter analyze and the 16 Dart tests
```

34 tests covering the sealed-key exchange, packet signing and tamper rejection,
duplicate-flood suppression, route preference, simulated radio range, kick thresholds and
re-keying, state persistence, GPS distance, Beacon v1 byte-exact round-trips and size
budget, pre-canned status codes, H3 cell snapping, zone votes counting people rather than
reports, contested cells resolving to unsafe, the bounded 16-entry re-gossip ring, radius
unit conversion, a node meshing over whichever of its radios still works, and the fact
that a relay outside a private network cannot read its SOS, status or zone traffic — plus
the duty-cycle ladder: that it eases off only when genuinely alone, snaps back on any
frame, never backs off during an SOS, and jitters without drifting the mean rate.
