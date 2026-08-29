# Architecture

How the Phase 1 terminal MVP is built, what the protocol looks like on the wire, what the
security model does and does not promise, and where Phase 2 plugs in.

```
crates/meshcli    terminal client: line parsing, tables, prompt
      | Command / Event channels
crates/meshcore
      node.rs       one task owns all state; the only place state is mutated
      net.rs        networks: keys, members, invites, kick tally, re-key
      router.rs     neighbours, learned routes, duplicate suppression
      packet.rs     Frame / Packet wire format, TTL, path recording
      beacon.rs     Beacon v1: the 27-byte BLE-advertisement format (core-only)
      status.rs     pre-canned panic codes, one byte each (core-only)
      zones.rs      H3 safe/unsafe zone aggregation and trust consensus
      battery.rs    platform battery read, cached
      crypto.rs     Ed25519, X25519 sealed boxes, ChaCha20-Poly1305, HKDF
      identity.rs   persistent UUID -> node id + keys
      store.rs      identity.json, contacts.json, networks.json, zones.json, messages/*.jsonl
      duty.rs       how hard to beacon and scan, given how alone we are
      transport/    the radio seam: udp.rs, external.rs (native BLE), multi.rs (all at once)
```

`meshcore` has no terminal code in it and `meshcli` has no protocol code in it. That
split is the Phase 2 prerequisite from [plan.md](../plan.md) §4: a Flutter or React Native
UI binds to the same `NodeHandle` (`Command` in, `Event` out) through `uniffi` or
`flutter_rust_bridge`, and every layer below is reused untouched.

## The node actor

`Node::spawn` starts one Tokio task that owns everything: contacts, networks, router,
outbox. Commands arrive over an `mpsc` channel with a `oneshot` reply; events flow back
over a second channel. Nothing else can touch the state, so there are no locks in the
protocol path and no chance of two UI actions racing.

Its `select!` loop services five things: an inbound frame, a user command, the 3-second
hello beacon, the 10-second latency ping, and a 5-second maintenance tick (prune dead
neighbours, flush contacts to disk, retry the outbox).

## Wire format

Two layers, both `bincode`-encoded:

```
Frame  { magic, version, link_from, packet }         one datagram, names the direct sender
Packet { id, origin, dest, sent_at_ms, body, sig,    end-to-end, signed by origin
         ttl, path }                                 ttl/path are rewritten by relays
```

`sig` is Ed25519 over `(id, origin, dest, sent_at_ms, body)`. `ttl` and `path` sit outside
the signature precisely because every relay must be able to change them; a relay cannot
touch anything else without invalidating the signature.

`Body` is one of:

| Body | Purpose |
| :--- | :--- |
| `Hello` | 3-second beacon: Ed25519 + X25519 public keys, self-chosen name, optional GPS, battery, SOS flag and current pre-canned status. Floods, so distant nodes learn keys they will need for invites. |
| `Ping` / `Pong` | Latency probe between direct neighbours. TTL 0 — never relayed. |
| `Envelope` | Any network traffic: `{network, epoch, nonce, ciphertext}`. |
| `Invite` | A network key sealed to exactly one recipient. |

Inside an `Envelope`, encrypted with the network key, is a `NetPayload`: `Chat`, `Direct`,
`Gps`, `Members`, `KickVote`, `Ack`, `Status`, `Sos` or `Zone`. A node that is not in the
network sees only the envelope — which network id, which epoch, and how big — and relays
it blind. That includes the emergency payloads: a relay carries someone's SOS without
being able to read it.

Putting SOS and status **inside the signature of `Hello`** is deliberate. A relay can
rewrite `ttl` and `path` and nothing else, so no intermediate node can clear someone's
SOS flag or forge a status on their behalf.

### Beacon v1 — the advertisement-sized format

`Frame` is `bincode` and runs to kilobytes. A legacy BLE advertisement carries 31 bytes of
AD payload, and a manufacturer-specific structure spends 2 on length+type and 2 on the
company id, leaving **27 usable bytes** — so [plan.md](../plan.md) §2's requirement that
presence, SOS, battery, GPS and heat-map data ride advertisements cannot be met by
`Frame` at any compression. `beacon.rs` is the second format: hand-packed, fixed layout,
no allocation.

```
header (4 bytes)   ver/type | flags (SOS, GPS, status, relay) | battery | seq
type 0 presence    node_id(8) lat_e7(4) lon_e7(4) status(1) hops(1) ttl(1)   = 23 bytes
type 1 zone        origin(8) cell(8) verdict(1) consensus(1) radius_m(2)     = 24 bytes
```

GPS is `i32` degrees × 1e7 (~1 cm, far finer than GPS itself). Phase 1 encodes, decodes
and tests this format against real node state; Phase 2 is what puts it on a radio, since
no laptop can advertise (see below). `beacon.rs` and `status.rs` import no `std`, so they
move into the `#![no_std]` core in Phase 3 unchanged.

## Routing

Path-recorded flooding with learned reverse routes:

1. **Dedupe.** Every packet has a random 128-bit id. The router keeps the last 4096 ids;
   a repeat is dropped immediately. This is what stops the broadcast storms flagged in
   [plan.md](../plan.md) §6.2.
2. **TTL.** Every relay decrements it; at 0 the packet dies. Default 8 hops.
3. **Route learning.** A packet that arrives from neighbour `N` having travelled `h` hops
   teaches us: *to reach `origin`, send to `N`, cost `h`*. Shorter routes replace longer
   ones; routes expire after 120s, neighbours after 30s of silence.
4. **Forwarding.** A packet addressed to a node we have a route for is **unicast** to that
   next hop. With no route it is **flooded** and the mesh sorts it out. Broadcasts always
   flood.
5. **Store-and-forward.** Direct messages go into an outbox and are re-sent (as fresh
   packet ids, so dedupe does not eat the retry) every 15s for 2 minutes, until the
   recipient's `Ack` comes back. A relay that walks away and returns still delivers.

Link quality is tracked per neighbour as measured RTT; the `rssi` field is carried and
displayed but stays empty on UDP, where there is no radio signal to read. A BLE transport
fills it in and the ranking in `--peers` starts using it.

`--peers` ranks by GPS distance first, then hops, then latency — the "nearest peers first"
requirement from [proposal.md](../proposal.md) — with ghosts always below reachable peers.

## Emergency signals

* **In-network SOS.** `--sos start` sets a flag that rides every `Hello` and sends an
  immediate `Sos` payload at **TTL 12** rather than the default 8: it is the one packet
  class allowed to be noisy, and the dedupe cache is what keeps that from becoming a
  storm. It is isolated from the operating system's emergency-call path by design
  ([plan.md](../plan.md) §3.2) so that testing a mesh can never dial real emergency
  services. Nothing in this codebase may wire it to one.
* **Pre-canned status.** Seven codes, one byte each (`status.rs`). The English text lives
  only in the renderer and never travels — that is the point, on a link with a 27-byte
  budget and a panicking user who should not have to type.
* **Ghosting.** A contact with no neighbour entry and no route is not deleted; it is shown
  dimmed at its last known GPS fix with the age of that fix. A dead battery must not look
  the same as never having existed.

## Safe and unsafe zones

Raw coordinates would not scale: a hundred people in one street each broadcasting a
distinct lat/lon is a storm with no useful aggregate at the end of it. So a report is
snapped to an **H3 cell** (resolution 8, ≈460 m edge — about a town block) and only the
cell travels: an 8-byte index, a verdict byte, and a 4-byte radius.

```rust
NetPayload::Zone { cell: u64, verdict: u8, radius_m: u32 }
```

### One bit, not a scale

Phase 1 carried a 0–4 safety level and averaged it across reporters. Phase 2B replaced it
with a single **safe / unsafe** verdict, for three reasons found by reading the question
out loud:

* **Nobody can answer a five-point scale under stress.** "Is this a 2 or a 3?" has no
  defensible answer at 3 a.m. in a flooded street. "Is it safe here — yes or no?" does.
* **The mean turned disagreement into a lie.** Two people reporting 4 and two reporting 0
  averaged to "2 — moderate": a sentence nobody said, painting a contested street amber.
* **The reported area was not the reporter's to choose.** The cell was fixed whether the
  person could vouch for their doorway or for the whole district. Now they say which, and
  the radius travels with the verdict — it is the only thing the cell cannot express.

### What makes the numbers mean something

* **One report per node per cell**, latest wins. Otherwise one node could shout a street
  safe fifty times and manufacture agreement.
* **Both vote counts travel separately**, never blended. [plan.md](../plan.md) §3.2
  requires a reader see how many people verified a zone *before* trusting the colour, so
  `--heatmap show` gives `SAFE` and `UNSAFE` their own columns and dims a lone report.
* **A tie resolves to unsafe.** `safe_votes > unsafe_votes` is the only way a cell reads
  safe. Painting a street green because two people disagreed with two others is the
  failure mode that gets somebody hurt, and the false alarms are worth it. The same rule
  decides draw order on the map: unsafe circles render on top, with a solid border.
* **The radius is the mean of the reports that *agree* with the verdict.** Averaging
  across disagreeing reporters would size the circle from people describing a different
  claim about the same ground.
* **Anything that is not an explicit `safe` byte decodes as unsafe.** A corrupted or
  future-versioned value must never be able to clear a hazard.

### Bounded re-gossip

Reports expire after 6 hours. Each node re-gossips **its own** reports, one cell per
5-second maintenance tick — never the aggregate, or the vote counts would compound as
reports bounce around the mesh. That round-robin is what lets a node that joins later, or
that was still starting up when a report went past, converge onto the whole map.

That ring is capped at **16 reports**, oldest evicted. Without a bound, a node that has
walked across a city re-gossips a hundred cells forever; the cap is also what makes the
structure portable to Phase 3, where it becomes a `heapless::Vec<Report, 16>` with a
compile-time capacity rather than an allocation.

Eviction stops a cell being *republished*. It does not withdraw the report — other nodes
are still counting that vote, and silently retracting it would rewrite their map from
under them. The report ages out on the same 6-hour TTL as everyone else's.

### Drawing it

The app renders each cell as a translucent circle of the reported radius, green for safe
and red for unsafe, at 0.18 alpha per contributing report and capped at 0.75. Overlap
density *is* the consensus signal — the darker the patch, the more people vouched for it —
and the cap exists because a fully opaque overlay hides the map underneath, which is how
somebody navigates out of the area.

## Every radio at once

Phase 2 made the radio a choice — the app started on Bluetooth *or* Wi-Fi, and changing
it stopped and restarted the node. Phase 2D removed the choice. `MultiTransport` is a
`Transport` over a set of `Transport`s:

```
send_broadcast  -> every child
send_to(addr)   -> the child that address arrived on, or every child if unknown
recv            -> whichever child speaks first
```

Routing, crypto, dedupe and the actor see one radio and did not change. A node reachable
on two radios is still one peer: the router keys neighbours by `NodeId`, and the duplicate
suppression that already handles a flooded packet arriving twice handles it arriving over
two radios for free.

**One dead radio must never take down the node.** `send_broadcast` succeeds if *any* child
accepted the frame; only a total refusal is an error. A phone with Bluetooth off meshes
over Wi-Fi, a Mac with no BLE peripheral role meshes over UDP, and neither is a failure
state — it is Tuesday. Constructing a node with *no* radios is the one hard error, because
there is nothing left to degrade to.

Each child gets a pump task feeding a shared channel rather than a `select!` over a
runtime-sized set of futures: `Transport::recv` is a single-consumer await, and cancelling
one branch of a `select!` mid-`recv` is how frames go missing.

## The duty cycle

A node alone was beaconing every 3 seconds and scanning at the radio's lowest-latency
setting forever — a flat battery by morning, spent on an empty room. `duty.rs` is a pure
function from conditions to a cadence:

| Alone for | Hello | Scan |
| :--- | :--- | :--- |
| peers present, or any SOS | 3 s | low latency |
| 0 – 1 min | 3 s | low latency |
| 1 – 5 min | 10 s | balanced |
| 5 – 20 min | 30 s | low power, 5 s window every 30 s |
| > 20 min | 60 s | low power, 5 s window every 60 s |

Four rules shape it, and each is an asymmetry rather than a preference:

* **The first minute alone is not solitude**, it is the join race. Backing off into it
  would make two phones started together slower to find each other, which is the one
  moment they must not be.
* **Any frame from anyone resets to the top rung** — including one we cannot decrypt or a
  version we do not understand. `last_heard_ms` is set in `on_frame` *before* the frame is
  parsed. Being slow to notice a rescuer costs more than a few beacons do.
* **An SOS never backs off**, ours or a peer's we are relaying. That is exactly the moment
  to spend the battery.
* **Scanning eases with the beacon.** Duty-cycling only the beacon — the obvious move —
  saves very little: a receiver listening at full tilt costs far more than a transmitter
  speaking occasionally.

Below 15 % charge the node drops one rung further. That is what finally makes the battery
byte in the beacon worth carrying.

Every interval is jittered by ±20 %. Twenty phones that started together would otherwise
beacon in lockstep forever and collide on air every time — worst exactly when the room is
fullest.

**iOS gets less out of this than Android.** CoreBluetooth has no scan-mode knob and no
advertising-interval control; the only lever is stopping and restarting the scan, so only
the window applies. Any drain measurement that assumes parity will be confusing.

## Identity and cryptography

* **Node id** — SHA-256 of a UUID generated on first launch, truncated to 8 bytes
  (16 hex characters). Not a MAC hash: modern operating systems randomise MAC addresses
  and hide them from userspace, so hashing one is neither stable nor available
  ([plan.md](../plan.md) §2, §6.4). The UUID and keys live in `identity.json`.
* **Signing** — Ed25519. Every packet is signed by its origin; receivers verify before
  acting, using the key learned from that node's `Hello`. Relays forward without
  verifying, since a relay may not know the origin's key yet.
* **Key agreement** — X25519. An invite is a sealed box: ephemeral X25519 key → shared
  secret → HKDF-SHA256 → ChaCha20-Poly1305. Only the invited node's private key opens it.
* **Network traffic** — ChaCha20-Poly1305 with the network's 32-byte symmetric key.
* **`[default]`** — uses a key derived from a published constant. It is deliberately
  **not private**: it is the lobby where everyone can be discovered. Treat anything you
  type there as public.
* **Epochs** — every network key has a generation number. A kick bumps it. Old keys are
  kept so packets already in flight still open, and the kicked node's key stops working on
  everything new.

### Kick voting

`--kick` broadcasts a signed ballot inside the network envelope. Every member tallies
independently; at `votes >= members / 2` the **lowest-id remaining member** — a
deterministic choice every member computes identically, with no host and no election —
generates a fresh key, seals it to each remaining member, and broadcasts the new member
list. The removed node keeps a key that opens nothing new. It is never notified.

### What this does not protect against

Honest limits of the MVP:

* **Traffic analysis.** `origin`, `dest` and the network id travel in clear so relays can
  route. Anyone in range learns who is talking to whom, and how often.
* **Data at rest.** `identity.json` and `networks.json` hold private and network keys
  unencrypted. Whoever can read the folder can read the networks.
* **Sybil / vote stuffing.** Identities are free to create. A member who invites five
  puppets to a network controls its kick votes. Real deployments need a cost or a
  vouching rule on invites.
* **Replay across epochs.** Old-epoch packets remain decryptable by design (in-flight
  delivery); a captured packet can be replayed to members until they forget the epoch.
* **The `[default]` network is public.** By design.

## Transports, and why not BLE

The `Transport` trait is three methods: `send_broadcast`, `send_to`, `recv`. Everything
above it is radio-agnostic.

Phase 1 ships `UdpTransport`: IPv4 multicast `239.42.13.7:47474` for discovery, limited
broadcast as a fallback, unicast for routed traffic, plus a set of addresses it has heard
from (so a single successful frame keeps a link alive even where multicast is filtered)
and any `--peer` seeds.

[plan.md](../plan.md) proposed BLE via `btleplug` for Phase 1. That is not buildable on
laptops: `btleplug` and its peers implement the BLE **central** role only — scanning and
connecting. Advertising as a **peripheral**, which every node must do to be discoverable,
is not exposed portably on macOS or Windows from userspace. A laptop mesh over BLE cannot
be assembled from the available libraries; a Wi-Fi mesh can, needs no infrastructure or
internet, and exercises exactly the same routing, crypto and CLI. So the MVP proves the
concept over UDP and leaves the radio swappable.

For Phase 2 that means:

* **Android** — Wi-Fi Aware / Wi-Fi Direct for bulk, BLE advertise+scan for discovery.
* **iOS** — Multipeer Connectivity, or CoreBluetooth peripheral + central.
* **Desktop** — the UDP transport keeps working, so a laptop stays a relay in a
  phone-dominated mesh.

Each is a `Transport` implementation. `node.rs`, `router.rs`, `net.rs`, `crypto.rs` and
the entire CLI do not change.

## Testing without three laptops

A node refuses to be confused about who it is: frames carry a random per-process
instance id alongside the node id, so a second process started on the same `--home` is
detected and reported instead of silently failing, and a packet claiming to originate
from us is dropped rather than turned into a contact.

Two mechanisms fake radio range:

* `--isolate <id...>` — a runtime link filter: the node drops frames from every node not
  listed, so you can build any topology with machines sitting on one desk.
* `--no-multicast --no-broadcast --peer <addr>` — the node only ever talks to the
  addresses you name, which is how [DEMO.md](DEMO.md) builds an A—B—C line inside one
  laptop.

`cargo test` (18 tests) covers the sealed-box exchange, packet signing and tamper
rejection, dedupe and route preference, the kick threshold and re-key, persistence across
restarts, Beacon v1 byte-exact round-trips and its size budget, status-code parsing and
the absence of human text on the wire, H3 cell snapping, zone votes counting people
rather than reports, and a relay outside a private network being unable to read its SOS,
status or zone traffic.
