# ReUnite — Phase Plan Index

This directory decomposes [`../plan.md`](../plan.md) into executable phases. One file per
phase. Each file is self-contained: goal, verified entry state, work items with the files
they touch, wire-format specs, acceptance criteria, and the deviations it accepts.

| Phase | File | Scope | Gate |
| :--- | :--- | :--- | :--- |
| 1 | [`phase-1-terminal-mvp.md`](phase-1-terminal-mvp.md) | Production-ready Rust core + terminal MVP: discovery, routing, private networks, SOS, panic codes, H3 heat map, ghosting | **complete** |
| 2 | [`phase-2-mobile.md`](phase-2-mobile.md) | iOS/Android app on the real core: native BLE transport, `dart:ffi` bridge, map/compass, SOS UI, background execution | steps 2.2–2.4 done; 2.1 radio half and 2.5 open |
| 2A | [`phase-2a-build-and-display-integrity.md`](phase-2a-build-and-display-integrity.md) | Restore the deleted build system, green `flutter analyze`, green `cargo test`, docs that match the code | **complete** |
| 2B | [`phase-2b-safe-unsafe-zones.md`](phase-2b-safe-unsafe-zones.md) | Binary safe/unsafe verdict over a user-entered radius; 16-report ring; overlapping translucent circles | **complete** |
| 2C | [`phase-2c-ble-interop.md`](phase-2c-ble-interop.md) | Android ↔ iOS Bluetooth discovery: why it never worked, and the fix | **fix landed, awaiting a two-phone test**; Beacon v1 on the air deferred |
| 2D | [`phase-2d-zero-touch-join.md`](phase-2d-zero-touch-join.md) | Every radio at once, adaptive duty cycle, laptops that join by being switched on | **complete**, except the battery measurement — moved to 2E |
| 2E | [`phase-2e-hardware-verification.md`](phase-2e-hardware-verification.md) | Run 2C and 2D on two real phones. **The only phase that cannot be done from a laptop**, and the handover document for whoever does | **next** |
| 3 | [`phase-3-embedded.md`](phase-3-embedded.md) | `no_std` core split, ESP32/nRF52 firmware, drone sky-relays and SOS triangulation | blocked on 2E |

> **Phases 2A–2E were inserted after Phase 2 shipped.** 2A–2D are complete and green;
> **2E is where the project actually is.** Everything about Bluetooth in 2C and 2D was
> established by reading code, never by running it on a radio — see
> [`phase-2e-hardware-verification.md`](phase-2e-hardware-verification.md), which is
> written as a handover for whoever has two phones.

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
| D7 | §3.2 One byte carrying the safety *average* of a hex grid | The scale answered a question people cannot answer under stress, and its mean blurred disagreement into a false amber | Replace with a verdict bit plus a 16-bit radius the reporter chooses. 3 bytes, still one packet per cell, still no raw coordinates | 2B |
| D8 | §3.2 Red/green *gradient* | The gradient implied a precision the input never had | Two colours. Density of agreement varies instead, by opacity of overlapping circles, with both vote counts shown as numbers | 2B |
| D9 | §2 Zero-config onboarding | Held, but the app asked which radio to use — a configuration question put to someone in an emergency | **Done in 2D.** Every radio starts at once; the picker is a status panel; permissions are requested lazily and a refusal degrades rather than blocks | 2D |

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
