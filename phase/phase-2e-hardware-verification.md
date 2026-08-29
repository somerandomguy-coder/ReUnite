# Phase 2E — Hardware verification and handover

> Inserted between Phase 2D and Phase 3. Goal: **find out whether any of 2C and 2D
> actually works**, on two real phones, and fix whatever the answer turns out to be.

**Entry condition:** 2A, 2B and 2D complete; 2C's fix landed.

**This phase is the only one in the project that cannot be done from a laptop.** It needs
one Android phone and one iPhone. Everything else has been done.

---

## For whoever picks this up

Read this section first. It is written so that a person or a model arriving cold can act
without re-deriving the last four phases.

### What this project is

An offline peer-to-peer emergency mesh. Every device is a node; it discovers neighbours,
relays for the ones it can hear, and carries chat, GPS, SOS, one-byte panic codes and
safe/unsafe zone reports across the resulting mesh. No accounts, no server, no internet.
Rust core (`crates/meshcore`), terminal client (`crates/meshcli`), C ABI bridge
(`crates/meshffi`), Flutter app (`mobile/`) with native BLE in Kotlin and Swift.

Start with [`../README.md`](../README.md), then
[`../docs/ARCHITECTURE.md`](../docs/ARCHITECTURE.md). The phase files in this directory
are the build plan; [`README.md`](README.md) indexes them and carries the deviations
register.

### What state it is in

| Layer | Status |
| :--- | :--- |
| Rust core: routing, crypto, zones, duty cycle | **built and tested**, 34 tests |
| Flutter UI on the real core over FFI | **built and tested**, 17 tests |
| Two nodes meshing over an in-memory transport | **tested** (`external_transport.rs`) |
| Two nodes meshing over an actual Bluetooth radio | **never once attempted** |

`./scripts/check.sh` runs everything that can be run here. It is green.

### The one thing to understand before changing any BLE code

**Nothing in the test suite touches a radio.** `crates/meshcore/tests/external_transport.rs`
exercises the BLE path through `ExternalTransport`, which is a pair of in-memory queues; a
Rust function called `pump()` shuttles frames from one queue to the other. That proves
everything *above* the radio is transport-agnostic. It says nothing about whether
CoreBluetooth and Android's BLE stack interoperate.

So: `swiftc -typecheck` passing means the Swift is valid, not that it works.
`flutter build apk` passing means the Kotlin compiles, not that it works. Every claim
about Bluetooth behaviour in phases 2C and 2D came from reading code and reasoning about
platform APIs. **Treat all of it as a hypothesis until this phase closes.**

### What was found in 2C, and why it matters here

The iPhone had **never started its Bluetooth radio**, from the first day the code was
written. `AppDelegate.swift` answered the `isEnabled` channel call from `self.ble?`, which
is nil until something calls `radio()` — so it returned false on every launch, and
`mesh_service.dart` bailed out *before* calling `start()`, showing the user a message
telling them to switch on a radio that was already on.

That has a consequence for this phase: **while that bug existed, no other iOS Bluetooth
defect could possibly have surfaced**, because nothing was running to fail. Four more were
found by inspection and fixed (reconnect storm on duplicate advertisements, dropped write
chunks, dropped notifications, unanswered batched writes). There may be more that only a
running radio will reveal. That is why §2E.2 is a stop-at-first-failure ladder and not a
checklist.

---

## 2E.1 — Build and install

The toolchain on the original developer's machine was already complete: `cargo-ndk`,
Android NDK 30, and the Rust targets `aarch64-linux-android`, `armv7-linux-androideabi`,
`x86_64-linux-android`, `aarch64-apple-ios`, `aarch64-apple-ios-sim`. Only the built
libraries were missing.

### Android — do this first, it is the shorter path

```bash
./scripts/build_ffi.sh android      # writes .so into mobile/android/app/src/main/jniLibs
cd mobile && flutter run -d <android-device>
```

- [ ] `mobile/android/app/src/main/jniLibs/` contains `libmeshffi.so` for each ABI.

**`jniLibs` is gitignored and starts absent.** A plain `flutter build apk` without this
step produces an app that compiles and then reports *"the mesh core did not start"*. That
is not a bug; it is a missing build step, and the startup screen now names it.

### iOS

```bash
./scripts/build_ffi.sh ios
```

Then two manual Xcode steps that cannot be scripted safely. Open
`mobile/ios/Runner.xcworkspace`:

- [ ] **Runner target → Build Phases → Link Binary With Libraries → + → Add Other…** →
      `mobile/ios/Frameworks/libmeshffi-device.a` (or `-sim.a` for the simulator).
- [ ] **Build Settings** → **Dead Code Stripping** = **No**, and add `-all_load` to
      **Other Linker Flags**.

Step 2 is not optional. Nothing in the Swift references the C entry points — Dart looks
them up at runtime with `dlsym` — so without it the linker strips
`mesh_start`, `mesh_command`, `mesh_ble_inject` and the rest, and the app shows the
startup-error screen.

---

## 2E.2 — The ladder

Both phones: **airplane mode on, Bluetooth on**, app open and **on screen**, within a few
metres of each other.

Run these in order. **Stop at the first one that does not happen** and go to the diagnosis
table below it. Each rung rules out everything above it.

- [ ] **1. Radio panel.** Networks tab on both phones. Both should read
      `Bluetooth state: on` and `Connected peers: 0`.
- [ ] **2. Android advertises.** `adb logcat -s ReUniteBle` shows
      `advertising as a mesh node`.
- [ ] **3. Android scans.** Same log shows `scanning for mesh peers`, and no
      `scan failed with code N`.
- [ ] **4. Each phone sees the other's advertisement at all.** Install **nRF Connect** on
      both and filter on `a1b2c3d4-e5f6-7890-1234-56789abcdef0`.
- [ ] **5. GATT connects and the service resolves.** Log shows `connecting to <id>` then
      `<id> is a mesh peer`.
- [ ] **6. A frame crosses.** The peer appears in the Peers list on at least one phone.
- [ ] **7. Both directions.** The roles are symmetric by design; verify it, do not assume.
- [ ] **8. A frame larger than one MTU crosses intact** — send a message over 500 bytes,
      which forces chunking and reassembly. This is the rung that exercises the iOS write
      flow-control fix from 2C.2.

**Rung 4 is the one that matters most.** It is the only test that separates *"the radios
cannot see each other"* from *"our code cannot see them"*, and those need completely
different fixes.

### Diagnosis

| Stops at | Most likely cause | Where to look |
| :--- | :--- | :--- |
| 1, says `unknown` for more than a second | The platform never pushed a state. The 2C.1 fix did not take. | `AppDelegate.swift:52` (`case "state"`), `BleMesh.swift` `onState`, `mesh_service.dart:338` (`case 'radio_state'`) |
| 1, says `off` or `unauthorized` | Genuine. The OS said so; this is not the app guessing. | Phone settings |
| 1, says `unsupported` on a phone that has Bluetooth | `isSupported` is wrong on this device | `MainActivity.kt` / `AppDelegate.swift:45` |
| 2, `advertising failed with code N` | 1 = data too large, 2 = too many advertisers, 4 = internal, **5 = no peripheral role on this chipset** | `BleMesh.kt` `startAdvertising`. Code 5 is a hardware limit: that phone can only ever be a central, and two such phones can never find each other. |
| 3, `scan failed with code 2` | App registration failed — on Android 12+ almost always the *Nearby devices* runtime permission | `AndroidManifest.xml` declares `BLUETOOTH_SCAN` with `neverForLocation`; check it was granted at runtime |
| 4, nRF Connect sees **neither** phone | Neither is advertising. Advertising is broken, not discovery. | `BleMesh.kt` `startAdvertising`, `BleMesh.swift` `peripheralManagerDidUpdateState` |
| 4, nRF Connect sees **both**, app sees neither | The radios are fine; **our scan or filter is wrong**. | `BleMesh.kt` `startScanning` (`ScanFilter`), `BleMesh.swift` `startScanning` (`scanForPeripherals(withServices:)`) |
| 4, sees Android but not iOS | Classic iOS symptom. Confirm the app is foregrounded — see the note below. | `BleMesh.swift` `peripheralManagerDidUpdateState` |
| 5, `connecting to <id>` repeats forever | Connection never completes. Check the 2C.2 throttle is actually in effect. | `BleMesh.swift` `connecting` map; `BleMesh.kt` `connectTo` |
| 5, `has no mesh service; disconnecting` | Service discovery found the device but not our GATT service | UUIDs must match across `BleMesh.kt`, `BleMesh.swift`, `transport/ble_linux.rs` |
| 6, connected but no peer appears | Frames are crossing the radio but not reaching the core | Check `mesh_ble_inject` is called: `mesh_service.dart` `case 'frame'`, then `crates/meshffi/src/lib.rs:388` |
| 8, short messages work, long ones do not | Chunking or reassembly. The iOS flow-control fix is the suspect. | `BleMesh.swift` `pumpWrites` / `pumpNotifications`, `FrameCodec.kt` reassembler |

> **A backgrounded iPhone is invisible to Android by design.** iOS moves 128-bit service
> UUIDs into the advertisement's *overflow area*, which non-Apple centrals cannot read.
> Any "it works until I lock the screen" result traces to this. It is an Apple platform
> constraint, not a bug in this code, and the fix is Phase 2 step 2.5 (background modes)
> plus 2C.4 (Beacon v1 in manufacturer data), not a change to discovery.

---

## 2E.3 — Verify 2D on the same two phones

Cheap once the phones are in hand, and none of it has been seen running either.

- [ ] **Both radios at once.** With Wi-Fi *and* Bluetooth on, both phones on one hotspot,
      the peer appears once — not twice. Dedupe is by `NodeId` in the router; two links
      delivering the same origin must not produce two peers.
- [ ] **One radio down.** Turn Bluetooth off on one phone. It must keep meshing over
      Wi-Fi, and the Radio panel must say why Bluetooth is gone. It must **not** show the
      startup-error screen.
- [ ] **Then the other.** Turn Wi-Fi off instead. Still meshing, over Bluetooth.
- [ ] **The duty cycle eases off.** Leave one phone alone for 25 minutes. `adb logcat`
      should show the cadence dropping through `balanced` to `low_power`. Bring the other
      phone into range: it must return to the fast rate within one interval.
- [ ] **An SOS never backs off.** Raise an SOS on the lone phone, wait past 5 minutes, and
      confirm the cadence stays at 3 s.
- [ ] **Measure the battery.** `plan.md` targets **< 5 %/hour idle**. Report the measured
      figure for: alone and backed off, one peer, five peers. **Publish the number even if
      it misses** — the target does not move to meet the measurement.

> **Expect Android and iOS to differ here, substantially.** CoreBluetooth has no scan-mode
> knob and no advertising-interval control; the only lever on iOS is stopping and
> restarting the scan, so only the *window* applies. A measurement that assumes parity
> will look like a bug and is not one.

---

## 2E.4 — What to fix next, once the ladder passes

In priority order. None of these should start before 2E.2 closes.

1. **Beacon v1 on the air** — [2C.4](phase-2c-ble-interop.md#2c4--beacon-v1-on-the-air--deferred-and-its-design-has-changed).
   Presence, SOS and battery currently need a full GATT connection, which is why two
   phones can be mutually invisible until one succeeds. `beacon.rs` has packed and
   round-trip tested the 27-byte format since Phase 1 with no radio to emit it.

   **Read the security constraint in that section before writing any code.** A `Frame` is
   Ed25519-signed and Phase 1 deliberately put SOS *inside* that signature so no relay can
   forge one. An advertisement has no signature and no room for one. A beacon may
   therefore only ever be a **discovery hint** that triggers a GATT connection; if it were
   allowed to set SOS state, anyone with a BLE radio could broadcast a forged SOS
   attributed to any node id, in an app whose entire premise is that an SOS is believed.

2. **Background execution** — Phase 2 step 2.5, untouched. Android foreground service with
   a persistent notification; iOS `CBCentralManager` state restoration. Today the mesh
   stops when the app leaves the screen, which is the single largest gap between this and
   something usable in a real emergency.

3. **Offline map tiles** — Phase 2 step 2.2. The interactive map fetches from
   `tile.openstreetmap.org`, so it is blank without internet in an offline-first app.
   Compass/Grid mode is the working fallback and is correctly the default tab; the map
   needs a bundled MBTiles pack.

4. **Three-phone relay** — Phase 2 acceptance criterion 3, never tested. A↔C only via B,
   verified by walking B out of range.

---

## Invariants — do not "simplify" these away

Each of these was a deliberate decision with a failure mode behind it. A future change
that looks like a cleanup can undo one without noticing.

| Invariant | Why | Where |
| :--- | :--- | :--- |
| SOS and status live **inside** the Ed25519 signature of `Hello` | So no relay can clear someone's SOS or forge a status on their behalf | `packet.rs`, `node.rs` |
| `ttl` and `path` sit **outside** the signature | Every relay must rewrite them and nothing else | `packet.rs` |
| An unsigned advertisement may never set peer state | Otherwise a forged SOS costs one BLE radio | 2C.4, `beacon.rs` |
| A zone tie resolves to **unsafe** | A contested area is not a safe area | `zones.rs` `Zone::verdict` |
| Anything that is not an explicit `safe` byte decodes as unsafe | A corrupt byte must never clear a hazard | `zones.rs` `Verdict::from_wire` |
| Both zone vote counts travel separately, never blended | "5 say safe" ≠ "5 say safe, 4 say unsafe" | `zones.rs`, `ZoneDto` |
| No automatic, unattended safety verdict anywhere | An earlier build auto-reported "safe" from GPS every 2 minutes, manufacturing false consensus | `mesh_service.dart` `_autoShareLocation` |
| Only a state the platform actually reported may accuse the radio | `unknown` ≠ "Bluetooth is off"; the previous build sent people to check a correct setting | `mesh_service.dart` `bleErrorForRadioState`, tested |
| One dead radio never takes down the node | A phone with Bluetooth off must still mesh over Wi-Fi | `transport/multi.rs` |
| Never back off the duty cycle during an SOS | That is the moment to spend the battery | `duty.rs`, tested |
| Compass/Grid is the **default** map view | A phone in a disaster has no tiles and no internet | `map_screen.dart` tab order, tested |
| The re-gossip ring is bounded at 16, and eviction does not withdraw the report | Other nodes are still counting that vote | `zones.rs` `record_own` |

---

## Acceptance criteria

1. Every rung of 2E.2 passes, or the one that fails is documented with its logcat output
   and a diagnosis.
2. 2E.3's radio-failover and duty-cycle checks pass on real phones.
3. A measured idle battery figure is published, whatever it is.
4. Phase 2 acceptance criteria 1, 2, 4 and 6 — which have been unverifiable since they
   were written — are marked pass or fail on evidence rather than on inspection.
5. This file records what was actually observed, so the next person does not have to run
   it again to find out.

## Risks

- **The fix may be incomplete.** 2C found five defects by reading code. Five is what
  inspection found; it is not necessarily what exists.
- **One phone may not be able to advertise at all.** Some budget Android chipsets have no
  BLE peripheral role (`advertising failed with code 5`). That phone can only be a
  central, and two such phones can never discover each other. Check this before concluding
  the code is wrong.
- **Do not fix by guessing.** Every rung has a log line. If a change is made without a
  failing rung pointing at it, it is a guess, and guesses in this layer are how the
  original bug survived from the first commit to phase 2C.
