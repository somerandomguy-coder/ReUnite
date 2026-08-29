# Running ReUnite on a laptop and on phones

The app is the **same mesh core** the `meshnet` terminal client runs, with a Flutter UI on
top. Routing, encryption, peer ranking, SOS, panic codes, ghosting and the safe-zone
consensus all happen in Rust; the UI only displays what the core reports.

```
Flutter UI  (mobile/lib)
    | dart:ffi, JSON in / JSON out
crates/meshffi   five C functions
    |
crates/meshcore  the mesh: routing, crypto, zones, SOS   <- identical to the CLI
    |
Wi-Fi (UDP)  or  Bluetooth LE  <- pick either in the app, on the Networks tab
```

**Two radios, one mesh.** Wi-Fi reaches laptops and works over any shared network,
including a hotspot with no internet. Bluetooth needs no infrastructure at all — two
phones in a field with everything else dead — but it is phone-to-phone only, because a
laptop cannot advertise as a BLE peripheral from userspace.

---

## 0. What you need once

```bash
# Rust (if you have not already)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Flutter 3.27+ — https://docs.flutter.dev/get-started/install
flutter doctor
```

For Android phones you additionally need Android Studio (for the SDK and an NDK), and:

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
cargo install cargo-ndk
```

For iPhones you need Xcode and:

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
```

---

## 1. Run it on your laptop (macOS)

```bash
git clone <this-repo> && cd ReUnite

# Build the Rust core and install it where the app looks for it
./scripts/build_ffi.sh macos

# Run the app
cd mobile && flutter run -d macos
```

The app generates an identity on first launch, joins `[default]`, and starts beaconing.
No account, no sign-up, no internet.

**If you see "The mesh core did not start"**, the library was not built — run
`./scripts/build_ffi.sh macos` and relaunch. The error screen shows every path it tried.

### Laptop app + laptop CLI on one machine

Useful for checking the app against a known-good node. The app uses port 47474, so give
the CLI a different port and point it at the app:

```bash
cargo build --release
./target/release/meshnet --home /tmp/nodeA --name laptop-cli \
  --port 47475 --peer 127.0.0.1:47474
```

Within a few seconds `--peers` in the CLI lists the app, and the app's **Peers** tab lists
the CLI.

---

## 2. Run it on an Android phone

```bash
./scripts/build_ffi.sh android      # builds arm64, armv7 and x86_64 .so files
cd mobile && flutter run -d <device-id>       # flutter devices to list them
```

`build_ffi.sh android` puts the libraries in `mobile/android/app/src/main/jniLibs/`, where
Gradle packages them into the APK automatically. Nothing else to configure.

To hand the app to someone else without a cable:

```bash
cd mobile && flutter build apk --debug
# mobile/build/app/outputs/flutter-apk/app-debug.apk
```

**Grant location permission when asked.** Android ties Wi-Fi and Bluetooth visibility to
location permission, and the GPS features need it anyway.

---

## 3. Run it on an iPhone

```bash
./scripts/build_ffi.sh ios
```

iOS then needs **two manual Xcode steps** that cannot be scripted safely:

1. Open `mobile/ios/Runner.xcworkspace`. Select the **Runner** target →
   **Build Phases** → **Link Binary With Libraries** → **+** → **Add Other…** and pick
   `mobile/ios/Frameworks/libmeshffi-device.a` (or `-sim.a` for the simulator).
2. **Build Settings** → set **Dead Code Stripping** to **No**, and add `-all_load` to
   **Other Linker Flags**. Without this the linker discards the C entry points, because
   nothing in the Swift code references them — Dart looks them up at runtime.

Then `flutter run -d <iphone>`.

> **iOS limitation, be aware of it.** Since iOS 14, multicast and broadcast require the
> `com.apple.developer.networking.multicast` entitlement, which Apple grants only on
> application. The app therefore disables multicast on iOS and relies on **broadcast plus
> explicitly added peers**. An iPhone will reliably reach nodes you add by IP; automatic
> discovery may not work until that entitlement is granted.

---

## 3.5 Two phones with no Wi-Fi at all (Bluetooth)

This is the case the whole project exists for: no router, no hotspot, no cell service.

1. Install the app on **both** phones (§2 for Android, §3 for iPhone).
2. Turn Bluetooth on. You can leave Wi-Fi and mobile data off entirely.
3. On each phone: **Networks** tab → under **Radio**, tap **Bluetooth**.
4. Grant the Bluetooth permissions when asked. Android asks for three separate ones
   (scan, advertise, connect) and refuses silently without all three.

Under **Radio** each phone then shows *Searching for other phones…* and, once they find
each other, *Connected to 1 phone(s) over Bluetooth. No Wi-Fi needed.*

Everything in §5 works identically from there: chat, positions, panic messages, SOS,
zones, ghosting. The mesh does not know or care which radio carried a frame.

**What is actually happening.** Each phone advertises the mesh service UUID and scans for
it at the same time, then connects both ways: it hosts a GATT server peers write frames
into, and connects out to peers as a client. Frames are length-prefixed and split across
BLE writes, because an advertisement holds 31 bytes and a mesh frame can be kilobytes.
The service UUID matches `crates/meshcore/src/transport/ble_linux.rs`, so a Linux laptop
running `meshnet --transport ble` joins the same mesh.

**Bluetooth range is short** — roughly 10–30 m through open air, much less through walls.
That is exactly why relaying matters: a third phone between two others extends the mesh,
and `--peers` will show the far one as `relayed` with 2 hops.

| Symptom | Fix |
| :--- | :--- |
| Stuck on "Searching for other phones…" | Both phones must be on the Bluetooth radio, both with the app open and on screen. Bring them within a few metres to pair up the first time. |
| "Bluetooth permission was refused" | Android: Settings → Apps → ReUnite → Permissions → *Nearby devices*. iOS: Settings → ReUnite → Bluetooth. |
| Android says advertising failed | Some older or budget chipsets cannot advertise as a peripheral. That phone can still receive by connecting outward, but two such phones cannot find each other. |
| Works, then stops when the screen locks | Expected today. Background execution is not implemented — see §7. |

## 4. Getting devices onto the same mesh

Every device must be on **the same Wi-Fi network**. No internet is needed — a router with
its uplink unplugged, or a phone hotspot with no data, both work fine. That is the point.

| Situation | What to do |
| :--- | :--- |
| Home/office Wi-Fi | Usually just works: discovery is multicast + broadcast |
| Wi-Fi that blocks multicast (hotels, campuses, many hotspots) | Add peers by IP (below) |
| No router at all | Turn on a phone hotspot and join every device to it |
| iPhone | Add peers by IP; see the entitlement note above |

**Finding a laptop's IP:** `ipconfig getifaddr en0` on macOS, `hostname -I` on Linux.

**Adding a peer by IP** — for the CLI, `--peer 192.168.1.42:47474`. All nodes use port
47474 by default, so two phones on one network find each other without any of this.

---

## 5. Test it, step by step

This is the sequence to prove every feature. Two devices minimum; three shows relaying.

### 5.1 They can see each other

Open **Peers** on both. Each should list the other within ~5 seconds, showing `direct`,
hop count, round-trip time and battery percentage.

> Nothing appears? Check both are on the same Wi-Fi, and see §6.

### 5.2 Messaging

On device A's **Chat** tab, type anything and send. It appears on B immediately, tagged
with the hop count. Tap a peer on the **Peers** tab → *Send a direct message* for a private
routed message; the sender sees a "delivered" line when the receipt comes back.

### 5.3 Positions and the compass

Tap the **crosshair** icon in Chat on both devices to share GPS. The **Peers** tab radar
now places each peer by real bearing and distance, and the list shows metres.

> There are no offline map tiles bundled, so this is Compass/Grid mode by design — it is
> what `plan.md` requires when tiles are unavailable, which in a disaster is the normal case.

### 5.4 Pre-canned panic messages

**Emergency** tab → tap **Need medical help**. It appears on the other device in amber, and
under that peer in the Peers list. Only a single byte crossed the network — the words are
reconstructed locally from a table that lives in the Rust core.

### 5.5 SOS

**Emergency** tab → drag the slider all the way right. On *every* device a red banner
appears at the top of the app, on every screen, and the peer turns red in the list and on
the radar. Clear it with *Stand down*.

> **The SOS is deliberately mesh-only.** It does not, and will not, call emergency
> services. `plan.md` §3.2 keeps the two apart precisely so that testing this app can
> never dial a real emergency line.

### 5.6 The safe-zone heat map and consensus

On device A: **Emergency** → set the slider to *Safe* → **Report this area**. A zone card
appears reading `4.0/4` and **1 report — unverified**.

Now do the same on device B, standing in the same place. Both devices update to show
`2 verifying` and the card brightens.

Report again from B and the count **stays at 2** — the consensus counts *people*, not
reports, so no single device can manufacture agreement. That is the whole reason the number
is shown separately from the colour.

### 5.7 Ghosting

Kill the app on device B (or turn its Wi-Fi off) and wait ~30 seconds. On A, B does not
disappear: it goes grey in the Peers list, marked `unreachable`, with *last seen at
&lt;coords&gt;* and how long ago. On the radar it becomes a hollow grey dot at its last
known position.

### 5.8 Private networks

On A: **Networks** tab → **+** → name it `rescue`. A switches to it automatically.
Copy B's node id from B's **Networks** tab (top of the screen), then on A tap
**Invite** and paste it. B gets a notice and can switch to `rescue`.

Messages in `rescue` are readable only by its members. A third device relays that traffic
without being able to decrypt it.

### 5.9 Multi-hop relaying

With three devices, walk device C out of A's Wi-Fi range but keep B in range of both. A
still sees C in **Peers**, now marked `relayed` with `2 hops`. Messages and SOS still get
through, because every node forwards for its neighbours.

---

## 6. When something does not work

| Symptom | Cause and fix |
| :--- | :--- |
| "The mesh core did not start" | The Rust library was not built for this platform. Run `./scripts/build_ffi.sh macos` / `android` / `ios`. |
| App runs, no peers ever appear | Devices on different Wi-Fi networks, or the network blocks multicast. Add the other device by IP. |
| Android sees nothing | Grant **location** permission — Android gates network discovery on it. The multicast lock is acquired automatically (see `MainActivity.kt`). |
| iPhone sees nothing | Expected without Apple's multicast entitlement. Add peers by IP. |
| Two nodes on one machine cannot see each other | They must use different ports **and** different home directories. The CLI warns when two processes share one identity. |
| `unsupported protocol version` | Mixed builds. Every device must run the same commit — the wire format is version 3. |
| macOS app cannot bind a port | Another node is already on 47474. Quit it, or run the CLI on `--port 47475`. |

---

## 7. What does not work yet

Being explicit, because the difference matters if you are testing:

* **Bluetooth is written but has not been run on real phones.** The Kotlin and Swift
  radios compile and the whole stack above them is tested — including two complete nodes
  meshing over a transport with no networking in it — but no physical phone-to-phone test
  has been done here, because no device was available. Treat §3.5 as the first real test
  of it rather than as a guarantee.
* **iOS needs the two manual Xcode steps in §3**, and they have not been performed here
  either. Without them Dart cannot find the core's symbols and the app shows the
  "mesh core did not start" screen.
* **No background execution.** Backgrounding the app stops the mesh. Android needs a
  foreground service; iOS has the CoreBluetooth background modes declared but no state
  restoration — Phase 2 step 2.5.
* **Beacon v1 is not on the air yet.** `crates/meshcore/src/beacon.rs` packs presence,
  SOS, battery and GPS into 27 bytes for BLE *advertisements*, so peers could be seen
  without connecting at all. Today the radio only advertises a service UUID and carries
  everything over GATT connections. Putting Beacon v1 into the manufacturer data is the
  next step and is what `plan.md` §2 ultimately asks for.
* **No offline map tiles.** Compass/Grid mode only, by design for now.
* **macOS runs unsandboxed in development.** `mobile/macos/Runner/*.entitlements` disable
  the App Sandbox so the app can bind a UDP port and load the core from `~/.reunite/lib`.
  This must be revisited before any signed distribution.

See [`phase/phase-2-mobile.md`](../phase/phase-2-mobile.md) for the full remaining scope.
