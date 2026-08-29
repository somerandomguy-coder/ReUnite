# Phase 1 — Terminal MVP (Laptop): Production-Ready Core

> `plan.md` §4 Phase 1. Goal: a Rust core that is complete enough that Phase 2 adds a UI
> and a radio, and nothing else.

Every capability in this phase lands in `meshcore` and is exposed through the existing
`Command` / `Event` / `Reply` seam. `meshcli` stays a thin shell — parsing and printing
only. If a feature needs protocol logic in `meshcli`, it is in the wrong crate.

---

## Entry state (verified against the code, not assumed)

| Step | Requirement | Status | Where |
| :--- | :--- | :--- | :--- |
| 1.1 | Hashed ID generation | `done` | `meshcore/src/types.rs` `NodeId::from_uuid`, `identity.rs` |
| 1.1 | Zero-config onboarding into `[default]` | `done` | `net.rs::NetworkBook::load`, `meshcli/src/main.rs` |
| 1.1 | Connectionless advertising/scanning | `partial` | UDP multicast + broadcast (`transport/udp.rs`); Linux BLE GATT (`transport/ble_linux.rs`); no advertising-packet format |
| 1.1 | Battery level telemetry | `todo` | — |
| 1.2 | Store-and-forward routing with TTL | `done` | `router.rs`, `node.rs::forward` |
| 1.2 | `clap` CLI | `done` | `meshcli/src/main.rs` |
| 1.2 | Binary-packed pre-canned messages | `todo` | — |
| 1.3 | X25519 key exchange / sealed invites | `done` | `crypto.rs::seal_to`, `node.rs::send_invite` |
| 1.3 | Decentralized kick voting + re-key | `done` | `net.rs::rekey`, `node.rs::on_kick_vote` |
| 1.4 | In-network SOS | `todo` | — |
| 1.4 | Last known location caching | `partial` | `Contact.gps` + `last_seen_ms` are persisted; nothing renders them as ghosts |
| 1.5 | H3 hex-grid aggregation | `todo` | — |
| 1.5 | Trust-consensus counting | `todo` | — |

Tests today: `meshcore/tests/mesh.rs` — 10 tests covering sealed boxes, envelope keying,
signature tamper rejection, flood dedupe, route preference, link filtering, kick threshold
and re-key, persistence of networks and identity, payload round-trips, haversine.

---

## Wire-format changes

### Protocol version bump: `VERSION` 2 → 3

`packet.rs::Hello` gains fields, so old and new builds cannot interoperate. `Frame::decode`
already rejects a mismatched version with a clear message, so this is safe — but every
laptop in a demo must run the same build. Called out here because it is the one change in
this phase that breaks compatibility.

### `Hello` beacon additions

```rust
pub struct Hello {
    pub ed_pub: [u8; 32],
    pub x_pub: [u8; 32],
    pub name: Option<String>,
    pub gps: Option<Gps>,
    // --- new in v3 ---
    pub battery: Option<u8>,   // 0..=100, None when the platform cannot report it
    pub sos: bool,             // in-network SOS flag (plan.md §3.2)
    pub status: Option<u8>,    // last pre-canned status code, so late joiners see it
}
```

### `NetPayload` additions

```rust
pub enum NetPayload {
    // ... existing: Chat, Direct, Gps, Members, KickVote, Ack
    Status { code: u8 },                  // pre-canned panic message, 1 byte on the wire
    Sos { active: bool, gps: Option<Gps> },
    Zone { cell: u64, level: u8 },        // one node's safety report for one H3 cell
}
```

`Status` carries a `u8`, never a `String` — that is the `plan.md` §3.2 requirement. The
human-readable text lives only in the renderer (`meshcli/src/render.rs`).

### Beacon v1 — the BLE-sized advertising format

`plan.md` §2 requires that routing pings, SOS, heat-map and GPS fit in BLE
manufacturer-specific data. A legacy BLE advertisement carries 31 bytes of AD payload; a
manufacturer-specific AD structure spends 2 on length+type and 2 on the company ID,
leaving **27 usable bytes**. `bincode` cannot hit that. So this phase adds a second,
hand-packed codec in a new `meshcore/src/beacon.rs`, independent of `Frame`.

Common header (4 bytes):

| Offset | Size | Field |
| :--- | :--- | :--- |
| 0 | 1 | `ver_type` — high nibble protocol version (`1`), low nibble beacon type |
| 1 | 1 | `flags` — bit0 SOS, bit1 has-GPS, bit2 has-status, bit3 relay-capable, bit4-7 reserved |
| 2 | 1 | `battery` — 0..=100, `0xFF` = unknown |
| 3 | 1 | `seq` — wraps; used to drop stale duplicates without a full 128-bit id |

**Type 0 — presence** (header + 19 = 23 bytes):

| Offset | Size | Field |
| :--- | :--- | :--- |
| 4 | 8 | `node_id` |
| 12 | 4 | `lat` — `i32`, degrees × 1e7 |
| 16 | 4 | `lon` — `i32`, degrees × 1e7 |
| 20 | 1 | `status` — pre-canned code, `0x00` = none |
| 21 | 1 | `hops` — hops travelled so far |
| 22 | 1 | `ttl` |

**Type 1 — zone** (header + 18 = 22 bytes):

| Offset | Size | Field |
| :--- | :--- | :--- |
| 4 | 8 | `origin` — reporting node id |
| 12 | 8 | `cell` — H3 cell index (`u64`) |
| 20 | 1 | `level` — aggregated safety, 0 (danger) … 255 (safe) |
| 21 | 1 | `consensus` — distinct reporters, saturating at 255 |

Both fit inside 27 bytes with room to spare. The node emits beacon types round-robin
across advertising intervals. Phase 1 encodes/decodes and unit-tests this format; Phase 2
is what actually puts it on a radio (see D2 in the [index](README.md)).

`beacon.rs` is written with **no `std` imports** (`core` + `alloc` only) so it survives the
Phase 3 crate split unchanged.

---

## Step 1.1 — Core node, discovery, battery telemetry

- [x] `meshcore/src/battery.rs`: `fn read_percent() -> Option<u8>`.
      macOS via `pmset -g batt` / IOKit, Linux via `/sys/class/power_supply/BAT*/capacity`,
      Windows stub returning `None`. Cached for 60 s — this must never be in a hot path.
- [x] `NodeConfig.battery_override: Option<u8>` and `meshnet --battery <0-100>`, so demos
      and tests are deterministic and a laptop on mains power can still show a number.
- [x] Include `battery` in every `Hello`; store on `Contact`; surface in `PeerView`.
- [x] `--peers` gains a `BATT` column; blank when unknown.
- [x] `beacon.rs` presence encode/decode, exact-byte unit tests.

## Step 1.2 — Routing and pre-canned panic messages

Routing is already done. This step is the binary-packed status codes.

- [x] `meshcore/src/status.rs`: the code table, `from_str`/`describe`, `no_std`-clean.

| Code | Name | Rendered |
| :--- | :--- | :--- |
| `0x00` | `none` | *(cleared)* |
| `0x01` | `safe` | I am safe |
| `0x02` | `medical` | Need medical help |
| `0x03` | `supplies` | Need water / food |
| `0x04` | `trapped` | Trapped — need rescue |
| `0x05` | `moving` | Moving to a safe zone |
| `0x06` | `shelter` | Shelter here, space available |
| `0x07` | `hazard` | Route blocked / hazard |

- [x] `Command::SetStatus { code: u8 }` → broadcasts `NetPayload::Status` on the active
      network **and** sets the field in subsequent `Hello`s, so a node that arrives later
      still learns it.
- [x] `Event::StatusUpdate { id, display, code }`; `render.rs` maps code → text.
- [x] CLI: `--status [code|name]` accepts `2` or `medical`. `--status` with no argument
      lists the table.
- [x] `Contact.status: Option<u8>` persisted; shown in `--peers`.

## Step 1.3 — Private networks

`done` — no new work. Verification only:

- [x] Confirm `Status`, `Sos` and `Zone` payloads are all sealed inside `Envelope` and
      therefore inherit private-network encryption. A node outside the network must see
      only ciphertext. Add a regression test asserting exactly that.

## Step 1.4 — In-network SOS and last-known-location ghosting

- [x] `Command::Sos { active: bool }`, CLI `--sos start` / `--sos stop`.
- [x] Sets `Node.sos`, which flips the `Hello` flag and the Beacon v1 SOS bit, and sends
      `NetPayload::Sos { active, gps }` immediately rather than waiting for the next beacon.
- [x] SOS packets get `ttl = 12` (vs. the default 8) and skip the outbox back-off — this is
      the one packet class allowed to be noisy.
- [x] `Event::SosRaised` / `Event::SosCleared`; rendered in red, and repeated in `--peers`
      with an `SOS` marker so it cannot scroll away.
- [x] **Isolation from OS SOS.** `plan.md` §3.2 is explicit: this never touches the
      platform emergency-call path. Add a comment saying so at the definition site and a
      line in `--help`, so nobody later "helpfully" wires it to `tel:911`.
- [x] Ghosting: `PeerView` gains `ghost: bool` (true when `last_seen_ms` is older than the
      neighbour timeout but a cached GPS fix exists). `--peers` prints ghosts dimmed with
      `last seen 45m ago` instead of dropping the row. `Contact` already persists
      `gps` + `last_seen_ms`, so this is a view change, not a storage change.
- [x] Ghost rows sort last, below live peers.

## Step 1.5 — Aggregated safe-zone heat map

- [x] Dependency check: `h3o` 0.11 has a toggleable `std` feature, so it is `no_std`-capable
      and is used with `default-features = false`. **The `grid.rs` fallback was not needed
      and was not written.**
- [x] Resolution **8** by default (≈0.46 km² per cell, ≈460 m edge) — a sensible "block"
      for a town. Configurable via `NodeConfig`.
- [x] `Command::ReportZone { lat, lon, level }`, CLI `--report-zone [lat] [lon] [level]`
      with `level` 0–4 (0 = dangerous, 4 = safe), scaled to the 0–255 wire byte.
- [x] `meshcore/src/zones.rs`: `ZoneBook` mapping `cell → { reports: HashMap<NodeId, u8>,
      last_update_ms }`. **One report per node per cell**, latest wins — that is what makes
      the consensus count meaningful and stops one node inflating a zone.
- [x] Aggregate = mean of the per-node levels; consensus = number of distinct reporters.
- [x] Persisted to `zones.json` in the node home, atomically like the other state files.
- [x] `Command::Heatmap` → `Reply::Heatmap(Vec<ZoneView>)`, CLI `--heatmap show`,
      rendered as a table: cell, centre lat/lon, level, **consensus count**, age.
      `plan.md` §3.2 requires the consensus count be displayed — the table shows it in its
      own column, never folded into the level.
- [x] Zones expire after 6 h and are pruned in the maintenance tick.
- [x] Beacon v1 type-1 encode/decode for zone gossip.

---

## Files this phase touches

```
new   crates/meshcore/src/beacon.rs      Beacon v1 pack/unpack (core+alloc only)
new   crates/meshcore/src/status.rs      pre-canned code table (core+alloc only)
new   crates/meshcore/src/zones.rs       H3 aggregation, consensus, persistence
new   crates/meshcore/src/battery.rs     platform battery read + override
edit  crates/meshcore/src/packet.rs      VERSION 3, Hello fields, NetPayload variants
edit  crates/meshcore/src/node.rs        Command/Event/PeerView, SOS state, handlers
edit  crates/meshcore/src/store.rs       Contact.status/battery, zones.json
edit  crates/meshcore/src/lib.rs         module wiring
edit  crates/meshcli/src/main.rs         --sos, --status, --report-zone, --heatmap, --battery
edit  crates/meshcli/src/render.rs       BATT/SOS/STATUS columns, ghosts, heatmap table
edit  crates/meshcore/tests/mesh.rs      new tests
edit  README.md, docs/ARCHITECTURE.md, docs/DEMO.md
```

## Acceptance criteria

1. `cargo build --release` and `cargo test` are green; no new warnings.
2. `--status medical` on node A shows on node B as *"Need medical help"*, and the packet
   on the wire carries a single byte — asserted by a test on the encoded `NetPayload`.
3. `--sos start` on A raises a red SOS on B and C, where C reaches A only through B.
4. Killing node C leaves it in `--peers` on A as a dimmed ghost with its last GPS fix and
   an age, not as a vanished row.
5. Three nodes reporting the same cell produce one heat-map row with `consensus = 3` and
   the mean level; a fourth report from an *already-counted* node updates the level and
   leaves `consensus` at 3.
6. A node outside a private network relaying a `Status`/`Sos`/`Zone` packet cannot read it
   — asserted by a test.
7. Beacon v1 encodes both types within 27 bytes, round-trips byte-exactly, and rejects
   truncated input.
8. `beacon.rs`, `status.rs` and `grid.rs` compile with no `std::` import (checked by
   inspection now; enforced by the Phase 3 split).
9. `docs/DEMO.md` gains an SOS + heat-map segment to the three-laptop script.

## Outcome

All nine acceptance criteria met; `cargo test` is 18/18 green and `cargo build --release`
is warning-free. Verified live on three nodes: a two-hop SOS, a ghost surviving the
neighbour timeout with its last fix, and a heat-map cell converging to `3.0/4` with
`consensus = 2` after one node reported twice.

Two things came out of the live runs rather than the design:

* **Zone reports needed periodic re-gossip.** A one-shot broadcast is lost to the
  startup race (a receiver drops packets from an origin whose key it has not learned yet)
  and is invisible to anyone who joins later. SOS and status were unaffected because they
  ride every `Hello`; the heat map had no such path. Each node now re-gossips **its own**
  reports, one cell per maintenance tick — never the aggregate, which would compound the
  consensus count as reports bounce around the mesh.
* **`grid.rs` was never needed** — see 1.5.

## Risks

- **Version bump breaks mixed-build demos.** Mitigation: `Frame::decode` already reports
  the mismatch clearly; the demo doc will say "same build on every machine".
- **`h3o` may not be `no_std`.** Mitigation is the `grid.rs` fallback above — decided
  before any code is written against it, not after.
- **Battery reading via shelling out to `pmset` is slow.** Mitigation: 60 s cache, and the
  read never blocks the actor loop.
- **SOS at TTL 12 in a dense room is a flood risk.** The dedupe cache bounds it, but the
  numbers should be re-checked with the three-laptop demo before Phase 2.
