# Project Plan: Offline P2P Emergency Mesh Network (Production-Ready)

## 1. Executive Summary
This document outlines the development plan for a production-ready, offline peer-to-peer (P2P) emergency communication application. The app enables users to communicate, share GPS locations, broadcast SOS signals, and map safe zones in environments without cellular or Wi-Fi infrastructure (e.g., natural disasters). 

The system will be designed for extreme battery efficiency and cross-platform compatibility. It features **zero-config onboarding** (no accounts, no internet required to start meshing) and is initially developed as a command-line interface (CLI) for laptops. It will scale to a mobile application (iOS/Android) and maintain a lightweight architecture capable of being ported to embedded systems (e.g., autonomous search-and-rescue drones).

## 2. Technology Stack & Rationale

To achieve high performance, low-level hardware access, multi-platform compatibility, and strict battery efficiency, we recommend a **Core-and-Shell architecture**. 

### Recommended Stack:
*   **Core Logic & Networking:** **Rust**
    *   *Why:* Rust offers memory safety, excellent performance, and zero-cost abstractions.
    *   *Libraries:* `RustCrypto` (for encryption/hashing) and `heapless` for embedded-safe data structures.
*   **CLI Application (Phase 1):** **Rust (with `clap` framework)**
*   **Mobile / Desktop GUI (Phase 2):** **Flutter** or **React Native** (using `flutter_rust_bridge` or `uniffi`).
*   **Data Storage:** **SQLite** (via `rusqlite`).

### Important Technical Considerations:
*   **Strict `no_std` Core vs. `std` Shell Architecture:** To support embedded drone deployments, the application uses a strict split:
    *   **The Core (`meshcore` - `#![no_std]`):** Completely OS-agnostic. Handles packet parsing, mesh algorithms, data structures, and cryptography. Compiles directly onto bare-metal microcontrollers (e.g., ESP32, nRF52).
    *   **The Shell (`meshcli` / Mobile UI - `std`):** The wrapper interacting with the OS. Handles hardware Bluetooth/Wi-Fi APIs, async runtimes (`tokio`), file storage, and the UI.
*   **Connectionless BLE (Advertising) vs. Paired Connections:** Establishing paired BLE connections between dozens of phones is flaky. **Strict requirement:** 90% of data (Routing pings, SOS, Heat Map, GPS) MUST be embedded directly into BLE Advertising Packets (Manufacturer Specific Data). Formal connections are ONLY established for the 3 seconds required to exchange a private network cryptographic key.
*   **Zero-Config Onboarding:** The app must generate a random UUID on launch, hash it, grant permissions, and instantly join the default mesh without any user sign-up or verification.

---

## 3. Architecture & Data Flow

### 3.1 Network Topology (Mesh)
*   **Nodes:** Every device (phone, laptop, drone) is a node acting as both client and router.
*   **Discovery & Telemetry:** Nodes continuously broadcast optimized BLE beacons containing their Hashed ID, SOS flag, and **Battery Level**.
*   **Routing:** Store-and-forward routing based on RSSI and traversal latency.

### 3.2 Key Features & Strict Data Packets
*   **In-Network SOS Signal:** An opt-in, high-priority packet that flips an SOS bit in the BLE beacon. Explicitly isolated from the OS hardware SOS to prevent false emergency service alarms outside the local mesh.
*   **Safe Zone Heat Map (H3 Hex Grids & Consensus):** To prevent broadcast storms, heat map data CANNOT be raw coordinates. **Strict requirement:** The app must aggregate data into low-resolution H3 hex grids. Nodes broadcast a single byte representing the safety average of their hex grid. The UI must display a **"Trust Consensus"** count (how many users verified the zone) before rendering the Red/Green gradient.
*   **"Last Known Location" Ghosting:** If a node drops off the network (e.g., battery dies), they do not disappear from the map. The UI must cache their last known GPS ping and timestamp, rendering them as a grayed-out "ghost" dot (e.g., *"Last seen here 45 mins ago"*).
*   **Pre-Canned Panic Messages:** To save BLE bandwidth and aid panicked users, the UI must provide large buttons for common updates ("I am safe", "Need Medical"). Under the hood, the Rust core must pack these as single-byte binary codes, NOT raw strings.
*   **Identities & Encryption:** Public Networks use unencrypted broadcasts. Private Networks use X25519 asymmetric encryption to securely exchange a ChaCha20-Poly1305 symmetric key.

---

## 4. Development Phases (Step-by-Step)

### Phase 1: Terminal MVP (Laptop) - Core Concept Proof
*Goal: Build the production-ready Rust core and prove connectionless mesh routing.*

*   **Step 1.1: Core Node & Discovery:** Implement connectionless BLE advertising/scanning, Hashed ID generation, and Battery Level telemetry.
*   **Step 1.2: Mesh Routing & CLI:** Implement store-and-forward routing (with TTL) and the basic `clap` CLI. Include binary-packing for Pre-Canned Messages.
*   **Step 1.3: Private Networks:** Implement X25519 key exchange and decentralized kick-voting.
*   **Step 1.4: In-Network SOS & Last Known Location:** Implement `--sos` and local caching of node timestamps.
*   **Step 1.5: Aggregated Heat Map:** Implement H3 hex-grid aggregation and consensus counting for `--report-zone`.

### Phase 2: Mobile Application (iOS/Android)
*Goal: Bring the mesh network to smartphones with a battery-efficient, user-friendly UI.*

*   **Step 2.1: Mobile BLE & Bridge Integration:** Because Rust BLE libraries struggle with mobile OS lifecycles, the BLE transport layer for mobile will be written using **native plugins (Swift/CoreBluetooth, Kotlin/Android BLE)**. The UI passes raw BLE packets through `flutter_rust_bridge` to the `no_std` core.
*   **Step 2.2: Map & Heat Map UI:** Implement an offline map view. **Strict requirement:** If offline map tiles are not downloaded, the UI must gracefully degrade to a "Compass/Grid Mode" showing relative distance and direction to peers.
*   **Step 2.3: SOS & Panic UI:** Implement a protected slide-to-activate SOS button and 1-tap Pre-Canned Panic Messages.
*   **Step 2.4: Chat & Network UI:** Build screens for private messaging, network management, and zero-config onboarding.
*   **Step 2.5: Background Optimization:** Leverage native mobile BLE layers for iOS CoreBluetooth background modes and Android foreground services.

### Phase 3: Embedded Integration (Drones & IoT)
*Goal: Port the core to autonomous systems for enhanced rescue operations.*

*   **Step 3.1: `no_std` Porting:** Ensure the core compiles entirely in `#![no_std]`.
*   **Step 3.2: Drone Node Deployment:** Flash the core onto ESP32/nRF52 radios on drones.
*   **Step 3.3: Aerial Relays & Search:** Drones act as sky-relays, expanding network range and triangulating SOS signals.

---

## 5. System Commands Reference (CLI Phase 1)

| Command | Description |
| :--- | :--- |
| `--create-network [name]` | Creates a private network and generates encryption keys. |
| `--network [name] --add [user_id]`| Invites a user securely via public key exchange. |
| `--kick [user_id]` | Initiates a decentralized vote to kick a user (requires >=50% consensus). |
| `--rename [id] [name]` | Creates a local alias for a user ID. |
| `--broadcast [msg]` | Sends a custom string message to the network. |
| `--status [code]` | Sends a pre-canned 1-byte binary message (e.g., `1` = Safe, `2` = Medical). |
| `--sos start` / `stop` | Toggles the high-priority In-Network SOS broadcast. |
| `--report-zone [lat] [lon] [lvl]` | Submits a safety report, automatically mapped to an H3 hex grid. |
| `--heatmap show` | Dumps the current aggregated hex grid safety zones and consensus counts. |
| `--network [name] --enable-storing`| Toggles local disk storage for messages in the specified network. |

## 6. Known Challenges to Address

1.  **The BLE Bandwidth Compromise (No Pictures):** BLE has microscopic payload sizes. We must explicitly ban sending images or voice notes. The app must be ruthlessly restricted to binary-packed data.
2.  **Heat Map Data Scaling:** Addressed by enforcing H3 grid aggregation. Raw coordinates for safety zones would crash the network.
3.  **Background Execution Limits:** Mobile OSes aggressively kill background apps. Strict native integration is required.
4.  **App Distribution (Offline):** Must implement a captive portal over Wi-Fi Direct so users can share the Android APK directly to victims who do not have the app installed.
