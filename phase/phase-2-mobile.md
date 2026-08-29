# Phase 2 — Mobile Application (iOS / Android)

> `plan.md` §4 Phase 2. Goal: the same `meshcore` running on a phone, with a native BLE
> radio underneath it and a UI that a frightened person can use in one tap.

**Entry condition:** Phase 1 accepted.

---

## Entry state

`mobile/` is a Flutter shell with **no connection to the mesh at all**:

| Piece | Status | Note |
| :--- | :--- | :--- |
| App skeleton, dark theme, 3-tab nav | `done` | `lib/app.dart`, `lib/shared/theme.dart` |
| Chat screen | `partial` | messages go into a local `List`, never onto a radio |
| Map screen | `partial` | one hard-coded peer, no map, no compass fallback |
| Networks screen | `todo` | placeholder text |
| `MeshService` | **mock** | hard-coded node id `565c7b6a…`, fake peer, no I/O |
| GPS | `done` | real `geolocator` fix with permission flow |
| `flutter_blue_plus` dependency | unused | declared in `pubspec.yaml`, never imported |
| Android permissions | `done` | `BLUETOOTH_SCAN/ADVERTISE/CONNECT`, fine+coarse location |
| iOS permissions | `partial` | Bluetooth + when-in-use location present; background modes missing |
| `MainActivity.kt` | bare | no platform channel |
| Rust ↔ Dart bridge | `todo` | neither `flutter_rust_bridge` nor `uniffi` is present |

---

## Step 2.1 — Bridge and native BLE transport

**Bridge choice: changed to a hand-written C ABI over `dart:ffi`, carrying JSON.**

The doc originally committed to `flutter_rust_bridge` 2.x. Building it out, three things
argued against it and none of them were visible from the plan:

* **It adds a build-time dependency for everyone.** `flutter_rust_bridge_codegen` has to
  be installed before anyone can rebuild the project, and its `cargokit` integration
  rewrites the Xcode and Gradle build. A teammate cloning this repo would hit that before
  they hit any mesh code.
* **The API surface is tiny.** Five functions. The generator earns its keep on a broad,
  evolving API; here it is machinery around `start`, `command`, `poll`, `free`.
* **`meshcore` already speaks `serde` everywhere**, so JSON costs nothing to produce and
  gives Dart a readable, stable contract - node ids as hex strings rather than byte arrays.

The `Command`/`Event`/`Reply` seam in `node.rs` was the actual prerequisite, and it did not
change. Swapping to `flutter_rust_bridge` later would touch only `crates/meshffi` and
`mobile/lib/bridge/`.

- [x] New crate `crates/meshffi` — the only crate that knows about FFI. Exposes
      `start(config) -> NodeHandle`, `call(Command) -> Reply`, and `events() -> Stream<Event>`.
- [x] Build targets: `aarch64-linux-android`, `armv7-linux-androideabi`,
      `aarch64-apple-ios`, `aarch64-apple-ios-sim`. `cargo-ndk` for Android.
- [x] **The radio lives in the platform, not in Rust.** Rust BLE libraries lose to mobile
      OS lifecycles. Implement `ExternalTransport` in `meshcore`: a `Transport` whose
      `send_broadcast`/`send_to` push bytes out through a callback into Dart, and whose
      `recv` is fed by frames Dart pushes in. Everything above it — routing, crypto, the
      actor — is reused untouched.
- [x] Android (`MainActivity.kt` + `BleMesh.kt`): `BluetoothLeAdvertiser` publishing
      Beacon v1 in manufacturer-specific data, `BluetoothLeScanner` with a low-latency
      scan filtered on the service UUID `a1b2c3d4-e5f6-7890-1234-56789abcdef0` (already
      used by `transport/ble_linux.rs`, so a Linux node and a phone interoperate), and a
      GATT server on the existing RX/TX characteristics for the frames too big to advertise.
- [x] iOS (`AppDelegate.swift` + `BleMesh.swift`): `CBPeripheralManager` advertising,
      `CBCentralManager` scanning, GATT for the same two characteristics.
- [~] **RSSI finally has a source.** Both scanners report it per advertisement; feed it to
      `Router::note_rssi`, which has existed and returned nothing useful since Phase 1.
- [~] Connections are opened **only** to exchange a network key, and closed immediately
      (`plan.md` §2: "3 seconds"). Everything else is connectionless advertising.

## Step 2.2 — Map and heat map UI

- [x] Replace the mock `MeshService` with a `ChangeNotifier` driven by the real event
      stream. Deleting the hard-coded peer is the acceptance test.
- [ ] Offline map tiles (`flutter_map` + a bundled/downloaded MBTiles pack).
- [x] **Graceful degradation is a hard requirement** (`plan.md` §4 step 2.2): with no
      tiles, the map becomes **Compass/Grid Mode** — a radar showing bearing and distance
      to each peer from the device heading. This is the default assumption, not the error
      case; a phone in a disaster will usually not have tiles.
- [x] Heat-map list: cells shaded red→green by level, **with the trust-consensus
      count drawn on the cell**. A cell with consensus 1 renders visibly weaker than one
      with consensus 10 — an unverified zone must not look like a verified one.
- [x] Ghost peers as grey dots with "last seen 45 mins ago".

## Step 2.3 — SOS and panic UI

- [x] Slide-to-activate SOS (never a plain tap — accidental activation is the failure mode).
- [x] A persistent, unmissable banner while SOS is active, and an explicit stop control.
- [x] On-screen text stating this alerts the local mesh **only**, not emergency services.
- [x] Large one-tap buttons for the Phase 1 status codes: I am safe / Need medical /
      Need water-food / Trapped / Moving / Shelter here / Hazard. One tap sends one byte.

## Step 2.4 — Chat, networks, onboarding

- [x] Zero-config first launch: generate identity, request permissions, land in
      `[default]`. No account, no sign-up, no network call.
- [x] Chat bound to `Event::Chat` / `Event::Direct` with real delivery state from
      `Event::Delivered`.
- [x] Networks screen (was a stub): create, invite by peer picker, storing toggle,
      member list, kick with the vote tally visible.
- [x] Local `--rename` aliases.

## Step 2.5 — Background execution

- [ ] Android: a foreground service with a persistent notification, plus battery-optimisation
      exemption prompt. Without this the OS kills the mesh within minutes.
- [ ] iOS: `bluetooth-central` + `bluetooth-peripheral` background modes in `Info.plist`,
      and `CBCentralManager` **state restoration** so the app rejoins after being jettisoned.
- [ ] Duty-cycle the advertising interval by battery level — this is what makes the
      battery byte in the beacon worth carrying.
- [ ] Measure. Target: < 5 %/hour battery drain while idle in the mesh.

---

## Open decisions for this phase

- **D4 (SQLite).** Decide here, with real data volumes: keep JSONL, or move history to
  `rusqlite` as `plan.md` specifies. The mobile chat screen is the first consumer that
  actually needs paged queries.
- **APK sideloading (`plan.md` §6.4).** A captive portal over Wi-Fi Direct to hand the APK
  to people who do not have the app. Real, valuable, and large. Recommend scoping it as a
  separate deliverable rather than smuggling it into 2.5.

## Acceptance criteria

1. Two physical phones, airplane mode with Bluetooth on, discover each other and exchange
   chat and GPS.
2. A phone and a Linux laptop running `meshnet --transport ble` interoperate.
3. Three phones relay: A↔C only via B, verified by walking B out of range.
4. `--peers`-equivalent list on the phone shows RSSI-derived proximity, nearest first.
5. With no map tiles installed the map screen shows Compass/Grid Mode, not an error.
6. SOS from a phone appears on the laptop CLI as an SOS event, and vice versa.
7. The app survives 30 minutes backgrounded on both platforms and still relays.
8. `MeshService` contains no mock data.


---

## Progress — step 2.1 partially done, 2.2/2.3/2.4 done over Wi-Fi

**Done and verified on this machine.**

* `crates/meshffi` - the C ABI bridge, 3 contract tests exercising the real entry points
  with real C strings.
* `mobile/lib/bridge/mesh_ffi.dart` - `dart:ffi` bindings with a documented library
  search path and a readable error when the core is missing.
* `MeshService` rewritten on the real core. **The mock is gone**: no hard-coded node id,
  no fake peer.
* All four screens: Chat, Peers (compass/grid + ghosts + battery + SOS), Emergency
  (slide-to-SOS, seven panic buttons sourced from the Rust table, zone reporter, heat map
  with consensus), Networks (create, invite, switch, storing, kick).
* `scripts/build_ffi.sh` for macOS / Android / iOS, and `docs/MOBILE.md`.
* Android multicast lock in `MainActivity.kt` - without it Android silently discards mesh
  beacons and the app looks broken for reasons that have nothing to do with the mesh.
* macOS entitlements: sandbox off for development, network client+server on.
* **13 Dart tests**, 7 of which pump the real widgets against a real running Rust node.
* **Verified live**: the macOS app meshed with a `meshnet` CLI node over UDP - 1 ms RTT,
  and the app's core recorded the peer's SOS, status code, battery and zone report.

**Step 2.1's radio half - written, compiles, untested on hardware.**

* `crates/meshcore/src/transport/external.rs` - a `Transport` with no I/O of its own, just
  an outbound queue the platform drains and an inbound channel it feeds. Bounded, so a
  radio that stops draining (Bluetooth off, permission refused) cannot grow it without
  limit. Bluetooth device ids are mapped to synthetic loopback addresses, deferring the
  `LinkAddr` refactor to Phase 3 rather than doing it mid-phase.
* FFI: `mesh_ble_drain`, `mesh_ble_inject`, `mesh_ble_peer_lost`, plus `mesh_stop` so the
  app can change radio without restarting.
* `BleMesh.kt` and `BleMesh.swift` - each device advertises **and** scans, hosts a GATT
  server **and** connects out as a client. Being symmetric means it does not matter who
  discovered whom. Frames are length-prefixed and chunked across writes, reassembled per
  device so two peers writing at once cannot corrupt each other.
* A radio picker on the Networks tab, with runtime Bluetooth permissions.

**Still open in this phase.**

* **No physical phone-to-phone test.** No device was available here. Everything above the
  radio is tested - including two complete nodes meshing over a transport with no
  networking - and both native layers compile, but the first real BLE test is still to come.
* **Beacon v1 is not on the air.** The 27-byte advertisement format exists and is tested,
  but the radio currently advertises only a service UUID and carries everything over GATT.
  Putting Beacon v1 into manufacturer data is what makes presence and SOS visible without
  connecting at all, which is what `plan.md` §2 ultimately asks for.
* **Connections are not torn down after key exchange.** `plan.md` §2 wants them held for
  ~3 seconds; today they persist, which costs battery.
* **RSSI** is read from scan results but not yet fed to `Router::note_rssi`.
* **Step 2.5 entirely.** No foreground service, no iOS state restoration. The iOS
  background modes are declared but nothing resumes the mesh.
* **Offline map tiles.** Compass/Grid mode is the only view.
* **iOS linking is manual.** `BleMesh.swift` is now in the Xcode project and compiles, but
  linking `libmeshffi.a` is still the two manual steps in `docs/MOBILE.md` §3.
