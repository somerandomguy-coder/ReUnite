# Phase 3 — Embedded Integration (Drones & IoT)

> `plan.md` §4 Phase 3. Goal: the same mesh core running bare-metal on ESP32/nRF52 radios,
> so a drone becomes a flying relay that extends the network over a landslide.

**Entry condition:** Phase 2 accepted.

---

## Step 3.1 — The `no_std` split

This is deviation **D1** from the [index](README.md), and it is the whole engineering
content of this phase. `meshcore` today depends on `tokio`, `socket2`, `std::fs`,
`std::net` and `serde_json`. None of that exists on a microcontroller.

Target layout:

```
crates/meshcore    #![no_std] + alloc   types, packet codec, beacon, status, grid,
                                        crypto, router tables, geo, zone aggregation
crates/meshnode    std                  the actor, tokio, store.rs, transports
crates/meshcli     std                  terminal client (depends on meshnode)
crates/meshffi     std                  mobile bridge (depends on meshnode)
crates/meshfw      no_std               ESP32/nRF52 firmware (depends on meshcore)
```

- [ ] Move the OS-facing modules out of `meshcore` into a new `meshnode`. `node.rs`,
      `store.rs`, `transport/`, `identity.rs` (it does file I/O) go; the rest stays.
- [ ] Replace `SocketAddr` in `router.rs` with a transport-agnostic `LinkAddr` — a BLE MAC
      is not a socket address, and pretending otherwise is why `ble_linux.rs` currently
      fabricates `127.0.0.1:47474` as a placeholder.
- [ ] Swap `HashMap`/`HashSet` for `heapless::FnvIndexMap` / `IndexSet` with compile-time
      capacities in the `no_std` core. The dedupe cache is currently 4096 ids; on an nRF52
      that is a budget decision, not a constant.
- [ ] `bincode` → a `serde` codec that works without `std` (`postcard`), or keep the
      hand-packed Beacon v1 as the only embedded format and leave `Frame` to the `std` side.
      Beacon v1 was designed in Phase 1 precisely so this option stays open.
- [ ] `ed25519-dalek`, `x25519-dalek`, `chacha20poly1305`, `sha2`, `hkdf` all support
      `default-features = false`. Verify each, and measure: signature verification on a
      Cortex-M4 at 64 MHz is milliseconds, and every relayed packet triggers one.
- [ ] CI job: `cargo build -p meshcore --target thumbv7em-none-eabihf`. This is the only
      thing that keeps the split from rotting.

## Step 3.2 — Drone node deployment

- [ ] Firmware crate `crates/meshfw` on `esp-hal` (ESP32) or `embassy-nrf` (nRF52).
- [ ] Beacon v1 straight into the radio's advertising payload — no framing layer.
- [ ] Identity from the chip's unique ID, hashed the same way `NodeId::from_uuid` does it,
      so a drone is indistinguishable from a phone to the rest of the mesh.
- [ ] Persist nothing but the identity. A drone is a relay, not a store.
- [ ] Power budget: advertising duty cycle tuned against the flight-controller's draw.

## Step 3.3 — Aerial relays and SOS triangulation

- [ ] Drone flies a lawnmower pattern logging `(own GPS, heard node id, RSSI, timestamp)`.
- [ ] Multilateration from RSSI at three or more known positions gives an SOS node's
      approximate location even when that node has no GPS fix of its own.
- [ ] Be honest about the error bars: RSSI-to-distance in a post-disaster environment
      (rubble, wet foliage, bodies of water) is worth tens of metres at best. The output is
      a search *area*, and the UI must render it as one — a confidence circle, never a pin.
- [ ] Drone dumps its log to the ground station over the same mesh on return.

---

## Acceptance criteria

1. `cargo build -p meshcore --target thumbv7em-none-eabihf` succeeds in CI.
2. `meshcli` and the mobile app run unchanged on top of the re-split crates.
3. An ESP32 dev board joins the mesh and relays between two laptops that cannot hear
   each other.
4. A drone pass over a known SOS node produces a location estimate, reported with its
   confidence radius.

## Risks

- **This phase is a refactor of everything below the actor.** It should not start until
  Phase 2 is stable, or it will be re-done.
- **Crypto cost on a microcontroller.** Ed25519 verification per relayed packet may force
  a design change (verify-on-delivery, not verify-on-relay). Measure before deciding.
- **Regulatory.** Flying a relay over a disaster area is an aviation and spectrum question
  before it is a software one.
