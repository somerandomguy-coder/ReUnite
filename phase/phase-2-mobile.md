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

**Bridge choice: `flutter_rust_bridge` 2.x.** It generates the Dart glue from Rust
signatures, supports streams (which maps directly onto the existing `Event` channel) and
does not need the UDL file that `uniffi` requires. The `Command`/`Event`/`Reply` seam in
`node.rs` was built for exactly this and does not change.

- [ ] New crate `crates/meshffi` — the only crate that knows about FFI. Exposes
      `start(config) -> NodeHandle`, `call(Command) -> Reply`, and `events() -> Stream<Event>`.
- [ ] Build targets: `aarch64-linux-android`, `armv7-linux-androideabi`,
      `aarch64-apple-ios`, `aarch64-apple-ios-sim`. `cargo-ndk` for Android.
- [ ] **The radio lives in the platform, not in Rust.** Rust BLE libraries lose to mobile
      OS lifecycles. Implement `ExternalTransport` in `meshcore`: a `Transport` whose
      `send_broadcast`/`send_to` push bytes out through a callback into Dart, and whose
      `recv` is fed by frames Dart pushes in. Everything above it — routing, crypto, the
      actor — is reused untouched.
- [ ] Android (`MainActivity.kt` + a plugin class): `BluetoothLeAdvertiser` publishing
      Beacon v1 in manufacturer-specific data, `BluetoothLeScanner` with a low-latency
      scan filtered on the service UUID `a1b2c3d4-e5f6-7890-1234-56789abcdef0` (already
      used by `transport/ble_linux.rs`, so a Linux node and a phone interoperate), and a
      GATT server on the existing RX/TX characteristics for the frames too big to advertise.
- [ ] iOS (`AppDelegate.swift` + a Swift plugin): `CBPeripheralManager` advertising,
      `CBCentralManager` scanning, GATT for the same two characteristics.
- [ ] **RSSI finally has a source.** Both scanners report it per advertisement; feed it to
      `Router::note_rssi`, which has existed and returned nothing useful since Phase 1.
- [ ] Connections are opened **only** to exchange a network key, and closed immediately
      (`plan.md` §2: "3 seconds"). Everything else is connectionless advertising.

## Step 2.2 — Map and heat map UI

- [ ] Replace the mock `MeshService` with a `ChangeNotifier` driven by the real event
      stream. Deleting the hard-coded peer is the acceptance test.
- [ ] Offline map tiles (`flutter_map` + a bundled/downloaded MBTiles pack).
- [ ] **Graceful degradation is a hard requirement** (`plan.md` §4 step 2.2): with no
      tiles, the map becomes **Compass/Grid Mode** — a radar showing bearing and distance
      to each peer from the device heading. This is the default assumption, not the error
      case; a phone in a disaster will usually not have tiles.
- [ ] Heat-map overlay: H3 cells shaded red→green by level, **with the trust-consensus
      count drawn on the cell**. A cell with consensus 1 renders visibly weaker than one
      with consensus 10 — an unverified zone must not look like a verified one.
- [ ] Ghost peers as grey dots with "last seen 45 mins ago".

## Step 2.3 — SOS and panic UI

- [ ] Slide-to-activate SOS (never a plain tap — accidental activation is the failure mode).
- [ ] A persistent, unmissable banner while SOS is active, and an explicit stop control.
- [ ] On-screen text stating this alerts the local mesh **only**, not emergency services.
- [ ] Large one-tap buttons for the Phase 1 status codes: I am safe / Need medical /
      Need water-food / Trapped / Moving / Shelter here / Hazard. One tap sends one byte.

## Step 2.4 — Chat, networks, onboarding

- [ ] Zero-config first launch: generate identity, request permissions, land in
      `[default]`. No account, no sign-up, no network call.
- [ ] Chat bound to `Event::Chat` / `Event::Direct` with real delivery state from
      `Event::Delivered`.
- [ ] Networks screen (currently a stub): create, invite by peer picker, storing toggle,
      member list, kick with the vote tally visible.
- [ ] Local `--rename` aliases.

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
