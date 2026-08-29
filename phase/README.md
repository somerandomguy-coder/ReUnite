# ReUnite — Phase Plan Index

This directory decomposes [`../plan.md`](../plan.md) into executable phases. One file per
phase. Each file is self-contained: goal, verified entry state, work items with the files
they touch, wire-format specs, acceptance criteria, and the deviations it accepts.

| Phase | File | Scope | Gate |
| :--- | :--- | :--- | :--- |
| 1 | [`phase-1-terminal-mvp.md`](phase-1-terminal-mvp.md) | Production-ready Rust core + terminal MVP: discovery, routing, private networks, SOS, panic codes, H3 heat map, ghosting | **next** |
| 2 | [`phase-2-mobile.md`](phase-2-mobile.md) | iOS/Android app on the real core: native BLE transport, `flutter_rust_bridge`, map/compass, SOS UI, background execution | blocked on 1 |
| 3 | [`phase-3-embedded.md`](phase-3-embedded.md) | `no_std` core split, ESP32/nRF52 firmware, drone sky-relays and SOS triangulation | blocked on 2 |

## Working protocol

1. Work proceeds **one phase at a time, in order**.
2. At the end of a phase: everything in its *Acceptance criteria* section is implemented,
   `cargo test` is green, and the phase file's checkboxes are ticked.
3. Work then **stops** and the phase is reported back for review.
4. The next phase starts only after explicit confirmation.

Phase 1 is large. If you would rather gate at finer granularity, its five steps
(1.1–1.5) are independently reviewable and can each be a checkpoint — say the word and
Phase 1 will report back per step instead of once at the end.

## Status legend used in the phase files

| Mark | Meaning |
| :--- | :--- |
| `done` | implemented and covered by a test |
| `partial` | implemented in a different form than `plan.md` describes, or missing a sub-requirement |
| `todo` | not started |

## Deviations register

`plan.md` states several requirements that the current code deliberately does not meet, or
cannot meet as literally written. Every one is recorded here so nothing is silently
dropped. Each has an owning phase.

| # | `plan.md` requirement | Reality | Resolution | Phase |
| :--- | :--- | :--- | :--- | :--- |
| D1 | `meshcore` is `#![no_std]` | It is emphatically `std`: `tokio`, `socket2`, `std::fs`, `std::net` | Split into `meshcore` (`no_std` + `alloc`: types, packet, codec, crypto, router tables, geo, grid) and `meshnode` (`std`: actor, store, transports). New Phase 1 logic is written std-free to keep the split cheap | 3 (prepared in 1) |
| D2 | 90% of data rides BLE advertising packets | Current `Frame` is `bincode` and up to 8 KB — two orders of magnitude past a 31-byte advert | Add a second, hand-packed **Beacon v1** wire format sized for manufacturer-specific data; keep the `Frame` format for connection-oriented traffic (chat, invites) | 1 (format), 2 (radio) |
| D3 | Laptops advertise over BLE | `btleplug` and friends are central/scanner-only on macOS and Windows; laptop-to-laptop BLE peripheral mode is not portable | Phase 1 ships UDP/Wi-Fi as the default transport behind the same `Transport` trait; the Linux `bluer` transport and `scripts/ble_gateway.py` stay as the BLE proof | 1 |
| D4 | Data storage is SQLite (`rusqlite`) | JSON + JSONL files under `~/.meshnet` | Keep files for Phase 1 — they are inspectable, atomic-written and tested. Revisit in Phase 2 when the mobile UI needs indexed history queries | 2 |
| D5 | Public networks are unencrypted broadcasts | `[default]` uses a well-known key derived from a constant | Keep. It is functionally public (the key is in the source) but routes public traffic through one code path instead of two, which removes a whole class of bug | — (accepted) |
| D6 | Battery level in every beacon | No battery telemetry anywhere | Add a `battery` provider with a `--battery` override so demos and CI are deterministic | 1 |

## Step numbering

`plan.md` was revised, and its Phase 1 steps were renumbered. The phase files use the
**new** numbering throughout:

| New | Old (pre-revision, still in some git history) |
| :--- | :--- |
| 1.1 Core node & discovery + battery | 1.1 Core node initialization & discovery |
| 1.2 Mesh routing & CLI + panic codes | 1.2 CLI interface & local state |
| 1.3 Private networks + kick voting | 1.3 Private networks / 1.4 Decentralized moderation |
| 1.4 In-network SOS & last known location | *(new)* |
| 1.5 Aggregated heat map | *(new)* |
