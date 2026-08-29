# Phase 2C — Android ↔ iOS Bluetooth interoperability

> Inserted between Phase 2 and Phase 3. Goal: an Android phone and an iPhone, both with
> the app open and no Wi-Fi anywhere, find each other and mesh.

**Entry condition:** Phase 2A accepted. 2B may run in parallel — they touch no common file.

---

## The reported symptom

> One Android 16 phone and one current iPhone. Both users open the app. Their networks
> never discover each other.

Phase 2 shipped `BleMesh.kt` and `BleMesh.swift` as *written, compiles, never run on
hardware*. This phase is the first time that claim is tested, and a code read found the
reason it could not have worked before anyone plugged in a phone.

---

## 2C.1 — Root cause: iOS never starts its radio

**This is the whole bug. Everything below it is a second-order defect that would not have
been reached yet.**

The start path is:

```
mesh_service.dart:224   _startBluetooth()
  → _ble.isSupported()  → AppDelegate.swift:44   result(true)          ✓ passes
  → _ble.isEnabled()    → AppDelegate.swift:46   result(self.ble?.isEnabled ?? false)
  → _ble.start()        → never reached
```

`self.ble` is `nil` until something calls `radio()`, and `radio()` is only called from
`"start"` and `"send"`. So `"isEnabled"` on a fresh launch reads `nil?.isEnabled ?? false`
and returns **false**, every time.

`mesh_service.dart:228` treats that as fatal, sets

```
_bleError = 'Bluetooth is turned off - switch it on to reach other phones'
```

and returns **before ever calling `start()`**. The iPhone never advertises, never scans,
and shows the user a message telling them to switch on a radio that is already on.

The bug is doubled: even with `self.ble` constructed, `BleMesh.swift:54` is

```swift
var isEnabled: Bool { central?.state == .poweredOn }
```

and `central` is itself only assigned inside `start()`. `isEnabled` is therefore
unanswerable before `start()` by construction — it is a question about a manager that does
not exist yet. Android has no such problem: `MainActivity.kt` routes `"isEnabled"` through
`radio()` and `BleMesh.kt` reads the system `adapter?.isEnabled`, which is true whenever
Bluetooth is on.

So: **the Android phone advertises and scans correctly into an empty room.** The iPhone is
not in it.

- [x] `AppDelegate.swift`: route `"isEnabled"` through `radio()`, not `self.ble?`.
- [x] **The contract inverted, which is the real fix.** Dart no longer asks. Both
      platforms *push* a `radio_state` event (`on` / `off` / `unauthorized` /
      `unsupported` / `resetting` / `unknown`) whenever the OS reports one — iOS from
      `centralManagerDidUpdateState`, Android from an `ACTION_STATE_CHANGED` receiver.
      CoreBluetooth is asynchronous: power state is not readable synchronously at any
      point a caller would like it to be, and every app that pretends otherwise has this
      bug.
- [x] `mesh_service.dart`: `start()` is called unconditionally, and the event
      subscription is established **before** it, so the first state reported is not lost.
- [x] The "Bluetooth is off" message may only appear when the platform has actually said
      so. `bleErrorForRadioState` maps only states the OS asserted; `unknown` and
      `resetting` produce nothing. Covered by a test — a diagnostic that fires when the
      code does not know is worse than none, because people believe it.

## 2C.2 — Defects behind it, in the order they will be hit

Each of these is real and each would have surfaced within minutes of 2C.1 being fixed.

- [x] **iOS reconnects on every duplicate advertisement.** `BleMesh.swift:250`
      `didDiscover` guards only on `peers[id] == nil`, but `peers` is not populated until
      `didConnect`. The scan runs with `CBCentralManagerScanOptionAllowDuplicatesKey: true`
      (`:232`), so every repeat advertisement — several per second — calls
      `manager.connect()` again for a peripheral whose connection is still in flight.
      Android already solved this with a 10-second `connecting` throttle
      (`BleMesh.kt` `connectTo`); iOS needs the same.
- [x] **iOS drops write chunks silently.** `BleMesh.swift:157` writes
      `.withoutResponse` in a loop with no regard for
      `peripheral.canSendWriteWithoutResponse`. CoreBluetooth discards writes queued past
      its limit and reports nothing. A frame chunked across five writes arrives truncated,
      the length-prefixed reassembler waits for bytes that will never come, and the peer
      looks connected while passing no traffic. Now queued per peripheral, gated on
      `canSendWriteWithoutResponse` and resumed from
      `peripheralIsReady(toSendWriteWithoutResponse:)`. `send()` reports frames
      **queued**, not delivered — claiming delivery is what let the old code report
      success for bytes it had thrown away.
- [x] **iOS drops notifications the same way.** `BleMesh.swift:142`
      `updateValue(...)` returns `false` when the transmit queue is full. The code records
      the failure and moves on; the chunk is gone. Now a single queue drained by
      `pumpNotifications()` and resumed from `peripheralManagerIsReady(toUpdateSubscribers:)`.
- [x] **iOS answers only the first batched write.** `BleMesh.swift:198` responds to
      `requests.first` only. `didReceiveWrite` can deliver several `CBATTRequest`s, and a
      central that gets no response to a `.withResponse` write stalls that link until it
      times out. Every request is now answered.
- [x] **RSSI is plumbed at last.** Both scanners now emit an `rssi` event per
      advertisement → `mesh_ble_rssi` → `ExternalTransport::note_rssi`, keyed by **device
      id**, because the scanner sees a device long before it knows which node is behind
      it. `Transport::rssi_for` is a new trait method (default `None`; UDP has no
      per-peer RSSI to give), and `Node::on_frame` attaches the cached reading to
      `link_from` at the one moment a frame names its sender. `Router::note_rssi` has
      existed and returned nothing useful since Phase 1.

## 2C.3 — Verify on the actual devices

The code read explains a total failure. It cannot rule out a *second* failure sitting
behind it, and Android 16 is new enough that its BLE behaviour deserves measurement rather
than assumption.

**None of these are ticked, and none can be from this machine — they need your two
phones.** The Radio panel on the Networks tab now reports each step's input, and
[`docs/MOBILE.md`](../docs/MOBILE.md) §3.5 carries the same ladder for whoever runs it.
Run in this order and stop at the first thing that does not happen:

- [ ] **Android advertises.** `adb logcat -s ReUniteBle` shows `advertising as a mesh
      node`, not `advertising failed with code N`. Code 1 is data-too-large, 2 is
      too-many-advertisers, 4 is internal, 5 is feature-unsupported — a device with no
      peripheral role reports 5 and can only ever be a central.
- [ ] **Android scans.** No `scan failed with code N`. Code 2 is app-registration-failed,
      which on Android 12+ almost always means `BLUETOOTH_SCAN` was never granted at
      runtime. `AndroidManifest.xml` declares it `neverForLocation`, which is correct and
      means Location Services need not be on — confirm that on a real Android 16 build
      rather than trusting the flag.
- [ ] **Each phone sees the other's advertisement at all.** Use nRF Connect on both:
      filter on `a1b2c3d4-e5f6-7890-1234-56789abcdef0`. This is the one test that
      separates "the radios cannot see each other" from "our code cannot see them".
- [ ] **GATT connects and the service resolves.** iOS `"<id> is a mesh peer"`, Android
      `"<id> is a mesh peer"`.
- [ ] **A frame crosses.** One `Hello` in, `mesh_ble_inject` called, the peer appears in
      the peers list.
- [ ] **Both directions.** The roles are symmetric by design; verify that, do not assume it.

## 2C.4 — Beacon v1 on the air — **deferred, and its design has changed**

Not implemented in this stage, deliberately. Two reasons, and the second one changes what
it should be built as.

**It should not go in front of a hardware test.** Everything in 2C.1 and 2C.2 fixes code
that has never run on a phone. Adding a new advertising payload — on both platforms, also
untested — to the same layer means the first real device test could fail for a brand new
reason, and the ladder in 2C.3 would not tell you which. Land the root-cause fix, verify
it on the two phones, then change the advertisement.

**Unsigned advertisements cannot carry authoritative state.** This is the part that was
not visible when 2C was written. A `Frame` is Ed25519-signed by its origin, and Phase 1
deliberately put SOS and status *inside* that signature so that no relay can clear
someone's SOS or forge a status on their behalf. A BLE advertisement has no signature and
no room for one — a 27-byte manufacturer-data field cannot hold a 64-byte Ed25519
signature, let alone the payload too.

So a Beacon v1 advertisement can only ever be a **discovery hint**: *a node with this id
is nearby, at roughly this signal strength*. It must not be allowed to set an SOS flag, a
battery level, a status code or a GPS fix in peer state, because anyone with a BLE radio
could then broadcast a forged SOS attributed to any node id they chose — in an app whose
entire purpose is that an SOS is believed.

The revised design, for whoever picks this up:

- [ ] Advertise Beacon v1 in manufacturer-specific data on both platforms, so a peer's
      **presence and proximity** are visible with no connection at all — which is the real
      win, and what makes discovery robust when GATT is failing.
- [ ] Treat the beacon's payload fields as a **hint that triggers a GATT connection**, not
      as state. The signed `Hello` that follows remains the only thing allowed to set SOS,
      status, battery or position.
- [ ] Document that split at the top of `beacon.rs`, or someone will later "optimise away"
      the connection and silently make forged SOS possible.
- [ ] Note for Phase 2.5 background work: **a backgrounded iOS app moves 128-bit service
      UUIDs into the advertisement's overflow area, which non-Apple centrals cannot see.**
      An Android phone cannot discover a backgrounded iPhone by service UUID. This is an
      Apple platform constraint, not a bug to fix, and any "it works until you lock the
      screen" report traces to it.

---

## Acceptance criteria

1. An Android 16 phone and a current iPhone, both in airplane mode with Bluetooth on and
   the app foregrounded, list each other as peers within 30 seconds.
2. A chat message crosses in both directions.
3. An SOS raised on either appears on the other.
4. A frame larger than one MTU crosses intact — verified by a message longer than 500
   bytes, which forces chunking and reassembly.
5. Peers list shows RSSI-derived proximity, nearest first.
6. A phone and a Linux laptop running `meshnet --transport ble` interoperate.
7. No diagnostic message claims Bluetooth is off when it is on.

## Outcome

Criteria 1–6 are **implemented but unverified**: they need two physical phones, and none
was available here. Criterion 7 is met and tested.

What was verified on this machine:

| Check | Result |
| :--- | :--- |
| `xcrun swiftc -typecheck` on `BleMesh.swift` (iOS SDK 26.5, arm64) | clean |
| `flutter build apk --debug` (compiles `BleMesh.kt`, `MainActivity.kt`) | built |
| `cargo test --workspace` | 28 passed |
| `flutter test` | 17 passed |
| `flutter analyze` | 0 issues |

That is the honest ceiling for this stage. It says the code compiles and the layers above
the radio still behave; it says nothing about whether two radios find each other, which is
what 2C.3 is for.

## Risks

- **A second failure may hide behind the first.** Nothing here has run on hardware. 2C.3
  is written as a stop-at-first-failure ladder for that reason, and the Radio panel exists
  so the first rung can be read without a laptop.
- **Two phones is not a mesh.** Three-device relaying is Phase 2 criterion 3 and is not
  re-tested here.
- **The Android APK builds without the core.** `libmeshffi.so` is gitignored, so a plain
  `flutter build apk` produces an app that compiles and then reports "the mesh core did
  not start". Run `./scripts/build_ffi.sh android` first.
