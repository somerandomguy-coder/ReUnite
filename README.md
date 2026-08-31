# 🚀 ReUnite — Off-Grid P2P Emergency Mesh Network

> **When cell towers collapse and power grids fail, ReUnite turns ordinary smartphones into decentralized, life-saving relay nodes—requiring strictly ZERO Wi-Fi, ZERO cellular service, and ZERO internet.**

---

## 🌟 Overview

In natural disasters—earthquakes, flash floods, hurricanes—communication networks are often the first infrastructure to fail. Cell towers collapse, power lines break, and thousands of survivors are cut off from emergency responders and loved ones.

**ReUnite** solves this critical problem by transforming everyday smartphones into autonomous, peer-to-peer (P2P) relay nodes. Using low-power **Bluetooth Low Energy (BLE)** and local **Wi-Fi radio signals**, devices form a resilient, self-healing mesh network in the air without touching a single server or cell tower.

---

## ✨ Key Capabilities

* 📶 **Zero-Infrastructure P2P Mesh**: Connects devices directly using Bluetooth Low Energy (BLE) and local Wi-Fi datagrams. No cell service, internet, or routers required.
* 🔄 **Multi-Hop Message Relaying**: Messages hop automatically from device to device ($A \to B \to C$) to extend communication range far beyond standard Bluetooth distance.
* 🚨 **One-Tap Emergency SOS & Status Alerts**: Instantly broadcast distress signals, pre-canned triage codes (*Medical*, *Trapped*, *Hazard*), and live battery levels to surrounding survivors and rescue teams.
* 🗺️ **Safe-Place Heatmaps & Offline Radar**: Aggregates community safety reports visually without central servers. Displays relative bearings and distances on a 100% offline compass radar.
* 🔒 **Local-First & Zero-Tracking**: Built on a zero-trust privacy model. No accounts, phone numbers, sign-ups, or remote tracking.

---

## 🛠️ Architecture & Tech Stack

ReUnite combines a high-performance **Rust** core engine with a responsive, cross-platform **Flutter** frontend:

| Component | Technology | Role & Function |
| :--- | :--- | :--- |
| **Mesh Core Engine** | **Rust** (`crates/meshcore`) | Cryptographic identity, epidemic packet routing, Uber H3 spatial indexing |
| **Mobile App** | **Flutter / Dart** (`mobile/`) | User interface, OpenStreetMap canvas, and offline radar visualization |
| **Native Radios** | **Swift (iOS)** & **Kotlin (Android)** | Low-level BLE peripheral advertising, GATT server, and scanning drivers |
| **FFI Bridge** | **C-ABI** (`crates/meshffi`) | High-speed C-bindings connecting Rust core directly into Flutter via `dart:ffi` |

---

## 📱 How to Install & Run ReUnite on Your Phone (Step-by-Step)

Follow this simple guide to install and run ReUnite on your Android or iPhone device.

---

### 📋 Prerequisites (One-Time Setup on Your Computer)

Before installing on your phone, install these two free tools on your computer:

1. **Rust Language**: Install from [rustup.rs](https://rustup.rs/).  
   *(On macOS / Linux terminal, run: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)*
2. **Flutter SDK**: Install Flutter from [flutter.dev](https://docs.flutter.dev/get-started/install).

---

### 🤖 Installing on an Android Phone

#### **Step 1: Enable Developer Options on Your Android Phone**
1. Open **Settings** on your phone.
2. Scroll down to the bottom and tap **About Phone** (or **System Info**).
3. Find **Build Number** and tap it **7 times in a row** until a popup says *"You are now a developer!"*.
4. Go back to **Settings -> System -> Developer Options**.
5. Scroll down and turn **ON** **USB Debugging**.

#### **Step 2: Connect Your Phone to Your Computer**
1. Plug your phone into your computer using a USB cable.
2. Unlock your phone. A popup will ask: *"Allow USB debugging?"*. Check **"Always allow"** and tap **Allow**.

#### **Step 3: Build & Launch the App**
Open a terminal on your computer and run:

```bash
# 1. Clone the repository
git clone https://github.com/somerandomguy-coder/ReUnite.git
cd ReUnite

# 2. Build the native P2P library for Android
./scripts/build_ffi.sh android

# 3. Launch the app onto your connected phone
cd mobile
flutter run
```

---

### 🍏 Installing on an iPhone (iOS)

#### **Step 1: Enable Developer Mode on Your iPhone**
1. Open **Settings** on your iPhone.
2. Tap **Privacy & Security**.
3. Scroll to the bottom and tap **Developer Mode**.
4. Toggle **Developer Mode ON** and restart your iPhone when prompted.
5. After your iPhone restarts, unlock it and tap **Turn On** to confirm.

#### **Step 2: Connect Your iPhone & Xcode**
1. Plug your iPhone into your Mac using a USB cable.
2. Unlock your iPhone and tap **Trust This Computer** when prompted.
3. Open **Xcode** on your Mac (available for free on the Mac App Store).
4. Open the iOS project: `open mobile/ios/Runner.xcworkspace`.
5. Select your iPhone as the build target in Xcode at the top toolbar. Under **Signing & Capabilities**, pick your Personal Apple ID team.

#### **Step 3: Build & Launch the App**
Open a terminal on your Mac and run:

```bash
# 1. Clone the repository
git clone https://github.com/somerandomguy-coder/ReUnite.git
cd ReUnite

# 2. Build the native P2P library for iOS
./scripts/build_ffi.sh ios

# 3. Launch the app onto your connected iPhone
cd mobile
flutter run
```

---

## 💻 Running a Terminal Node (Linux / macOS / Windows)

You can also run ReUnite as a command-line mesh node on any laptop or desktop:

```bash
# Build and run a terminal node
cargo run --package meshcli -- --name My-Laptop-Node
```

Useful terminal commands once started:
* `--peers` : Show all reachable mesh peers, battery levels, and distance.
* `--routes` : View learned mesh multi-hop paths and next hops.
* `--sos start` : Broadcast an emergency SOS beacon to nearby nodes.

---

## 🧪 Testing & Verification

ReUnite includes full automated unit and integration tests across both Rust engine and Flutter frontend:

```bash
# Run the complete automated check suite
./scripts/check.sh
```

---

## 📄 License

Built for disaster resilience, emergency coordination, and humanitarian open-source technology. Released under the MIT / Apache 2.0 license.
