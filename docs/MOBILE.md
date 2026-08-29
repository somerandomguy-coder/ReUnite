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
3. Grant the Bluetooth permissions when asked. Android asks for three separate ones
   (scan, advertise, connect) and refuses silently without all three.

There is **no radio to pick**. The app starts every radio the phone has and uses all of
them at once; refusing the Bluetooth permission costs Bluetooth, not the mesh, and the
Radio panel says so.

The **Radio** panel on the Networks tab reports what the platform has actually told the
app, one fact per line:

```
Bluetooth radio is on
  Reported state    on
  Connected peers   0
```

`Reported state` is the operating system's own answer, not a guess — that distinction
matters, and §6 explains why. `Connected peers` is how many peers a frame could reach
right now. When it is `0` and the state is `on`, the radio is working and has simply not
found anybody yet.

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

### If they do not find each other

Work down this list and stop at the first step that does not do what it says. It is
ordered so that each step rules out everything above it.

| # | Check | What it means |
| :--- | :--- | :--- |
| 1 | **Radio** panel says `Reported state: on` on *both* phones | If it says `off`, `unauthorized` or `unsupported`, fix that first — the app is repeating what the OS told it. If it says `unknown`, the OS has not answered yet; give it a second. |
| 2 | Android logcat shows `advertising as a mesh node` | `adb logcat -s ReUniteBle`. `advertising failed with code 5` means the chipset has no peripheral role; that phone can still connect outward, but two such phones can never find each other. |
| 3 | Android logcat shows no `scan failed with code N` | Code 2 is app-registration-failed, which on Android 12+ almost always means the *Nearby devices* permission was never granted at runtime. |
| 4 | A third-party scanner sees each phone | Install **nRF Connect** on both and filter on `a1b2c3d4-e5f6-7890-1234-56789abcdef0`. This is the one test that separates "the radios cannot see each other" from "our code cannot see them". |
| 5 | The log shows `<id> is a mesh peer` | The GATT connection resolved the mesh service. If you get `connecting to <id>` and never this line, the connection is failing after discovery. |
| 6 | `Connected peers` goes above 0 | Frames can now cross. Anything still broken is above the radio. |

Other symptoms:

| Symptom | Fix |
| :--- | :--- |
| Peers found, but nothing arrives | Both phones must be on the **same network** — the `[default]` lobby unless you switched. Check the Networks tab. |
| "Bluetooth permission was refused" | Android: Settings → Apps → ReUnite → Permissions → *Nearby devices*. iOS: Settings → ReUnite → Bluetooth. |
| Works, then stops when the screen locks | Expected today. Background execution is not implemented — see §7. |
| An iPhone is invisible to an Android phone while backgrounded | Not fixable in this app. Backgrounded iOS moves 128-bit service UUIDs into the advertisement's *overflow area*, which non-Apple scanners cannot read. Keep the app on screen. |

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

### 5.6 Safe and unsafe zones

On device A: **Emergency** → **Report the area around you**.

1. Tap **SAFE** or **UNSAFE**. There is no scale — the question is deliberately one you
   can answer without thinking about it.
2. Type a length in the field and pick a unit: metres, kilometres, feet or miles. The line
   under the field tells you what you are about to claim, and reminds you that your exact
   position is snapped to a hex cell before anything is sent.
3. **Report this area**.

A zone card appears reading `UNSAFE` · `within 750 m`, with two chips — `0 safe` and
`1 unsafe` — and the label **unverified**, because one person is not a consensus.

Now report the *opposite* verdict from device B, standing in the same place. Both devices
show `1 safe / 1 unsafe` and the card stays **UNSAFE**, now labelled **contested**. That
is the rule: a cell reads safe only when more people vouch for it than against it, and a
tie resolves to unsafe. Nothing is averaged into an amber middle.

Report again from B and the counts **do not move** — votes count *people*, not reports, so
no single device can manufacture agreement.

On the **Peers → Interactive Map** tab, the zone is a translucent circle of the radius you
gave, red for unsafe and green for safe. Report the same area from a third device and the
circle visibly darkens: overlap density is the consensus signal, and the legend bottom-left
shows what 1, 3 and 5+ reporters look like. It never reaches full opacity, because the map
underneath is how somebody navigates out.

> **Nothing files a safety report on your behalf.** The app shares your *position*
> automatically every two minutes so peers can place you, but a safety verdict is only
> ever sent when a person taps the button. An earlier build auto-reported "safe" from GPS
> every two minutes; that manufactured exactly the false consensus the vote counts exist
> to prevent.

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
* **RSSI is now plumbed**, so the peers list ranks by real signal strength on Bluetooth.
  It has no source on Wi-Fi and stays blank there — Wi-Fi RSSI belongs to the
  association, not to a peer.
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
