# Project Plan: Offline P2P Emergency Mesh Network

## 1. Executive Summary
This document outlines the development plan for an offline peer-to-peer (P2P) emergency communication application. The app enables users to communicate and share GPS locations in environments without cellular or Wi-Fi infrastructure (e.g., natural disasters). It will initially be developed as a cross-platform command-line interface (CLI) for laptops, with a clear architectural path toward a mobile application (iOS/Android).

## 2. Technology Stack & Rationale

To achieve the goals of high performance, low-level hardware access (Bluetooth Low Energy/Wi-Fi Direct), cross-platform compatibility (Desktop + Mobile), and strong security, we recommend a **Core-and-Shell architecture**.

### Recommended Stack:
*   **Core Logic & Networking:** **Rust**
    *   *Why:* Rust offers memory safety, excellent performance, and compiles natively to all target platforms (macOS, Windows, Linux, iOS, Android). It is the best choice for building the complex, stateful mesh routing and cryptographic core.
    *   *Libraries:* `btleplug` (for cross-platform BLE), `RustCrypto` or `ring` (for encryption/hashing), `tokio` (for asynchronous runtime).
*   **CLI Application (Phase 1):** **Rust (with `clap` framework)**
    *   *Why:* Keeps the Phase 1 implementation in a single language. `clap` provides a robust, easy-to-use parser for the complex terminal commands required.
*   **Mobile / Desktop GUI (Phase 2):** **Flutter** or **React Native**
    *   *Why:* Allows for a single codebase for the UI on Android, iOS, Windows, macOS, and Linux.
    *   *Integration:* Use tools like `flutter_rust_bridge` (for Flutter) or `uniffi` to seamlessly connect the mobile UI to the Rust core.
*   **Data Storage:** **SQLite** (via `rusqlite`) or a lightweight key-value store like `sled`.
    *   *Why:* For persisting encrypted messages locally when `--enable-storing` is activated, and for storing address books (ID to Name mappings).

### Important Considerations for Mobile:
*   **MAC Address Randomization:** Modern operating systems (iOS, Android, Windows) randomize MAC addresses for privacy and restrict direct access to the hardware MAC. Relying strictly on the hardware MAC address is unreliable. *Recommendation:* Generate a persistent random UUID on first launch, hash it, and use this as the Node ID.
*   **Hardware APIs:** While BLE is universally supported, high-bandwidth P2P differs by OS (Apple uses Multipeer Connectivity; Android uses Wi-Fi Direct/Aware). The Rust core will need platform-specific adapters to utilize Wi-Fi for larger data transfers, falling back to BLE for discovery and small text/GPS packets.

---

## 3. Architecture & Data Flow

### 3.1 Network Topology (Mesh)
The system will operate as a decentralized mesh network.
*   **Nodes:** Every device is a node that acts as both a client and a router.
*   **Discovery:** Nodes continuously broadcast a BLE beacon containing their Hashed ID.
*   **Routing:** To find the most efficient route, nodes will track the Received Signal Strength Indicator (RSSI) of neighbors (to approximate distance) and measure message traversal latency.

### 3.2 Security & Privacy
*   **Identities:** ID is a cryptographic hash (e.g., SHA-256) of a locally generated UUID.
*   **Encryption:** The `[default]` network is unencrypted (or uses a known shared key) for public broadcasts. Private networks use asymmetric encryption (e.g., X25519 for key exchange) to securely share a symmetric key (e.g., ChaCha20-Poly1305) for that specific network. Only members with the private network key can decrypt messages.

---

## 4. Development Phases

### Phase 1: Terminal MVP (Laptop)
*Goal: Prove the P2P concept and mesh routing via a laptop terminal interface.*

**Step 1.1: Core Node Initialization & Discovery**
*   Implement BLE advertising and scanning.
*   Generate the Hashed ID.
*   Implement the `[default]` network state. Devices can see each other and exchange basic ping/GPS packets.

**Step 1.2: The CLI Interface & Local State**
*   Implement the command parser.
*   Implement `--rename [ID] [name]`: Store this mapping in a local SQLite database or JSON file.

**Step 1.3: Private Networks & Cryptography**
*   Implement `--create-network [name]`: Generates a new cryptographic keypair for the network.
*   Implement `--network [name] --add [user]`: The host encrypts the network's shared symmetric key using the invited user's public key (retrieved via the default network) and sends it to them.
*   Implement `--network [name] --enable-storing`: Write incoming/outgoing messages for this network to the local database.

**Step 1.4: Decentralized Moderation (Voting)**
*   Implement the kick voting mechanism. When a user issues a kick request, it broadcasts a signed vote to the network. Nodes tally the votes. If `votes >= (network_size / 2)`, the network generates a new shared key, distributes it to all remaining users, effectively locking out the kicked user.

**Step 1.5: Mesh Routing Logic**
*   Implement the store-and-forward mechanism. If User A wants to reach User C, but is only connected to User B, User B relays the encrypted packet.
*   Implement the routing table based on RSSI and latency.

### Phase 2: Mobile Application (Future)
*Goal: Bring the mesh network to smartphones with a user-friendly UI.*

*   **Extract Core:** Ensure the Rust core is fully decoupled from the CLI interface.
*   **Bridge:** Setup `flutter_rust_bridge` (if using Flutter).
*   **UI Implementation:** Build screens for Map (GPS view), Chats, Network Management, and Settings.
*   **Platform Specifics:** Implement background processing permissions (CoreBluetooth on iOS, Bluetooth/Location permissions on Android).

---

## 5. System Commands Reference (CLI)

Upon launching the application, the prompt will display the current active network, e.g., `[default] >`.

| Command | Description |
| :--- | :--- |
| `--create-network [name]` | Creates a new private network and switches context to it (e.g., `[name] >`). Generates network encryption keys. |
| `--network [name] --add [user_id]` | Invites a user (by their Hashed ID) to the specified private network. Securely exchanges keys. |
| `--network [name] --enable-storing`| Toggles local disk storage for messages in the specified network. |
| `--kick [user_id]` | Initiates or adds a vote to kick a user from the current private network. (Requires >=50% consensus). |
| `--rename [id] [name]` | Creates a local alias for a user ID. The UI/CLI will display `[name]` instead of the hash. |
| `--switch [network_name]` | Switches the active CLI context to a different network. |
| `--broadcast [message]` | Sends a message to all users in the current network context. |

## 6. Known Challenges to Address
1.  **Background Execution:** Mobile OSes aggressively kill background apps to save battery. The BLE beaconing must be carefully designed to operate within OS constraints (e.g., iOS CoreBluetooth background modes).
2.  **Mesh Network Flooding:** In dense areas, broadcast storms can occur. The routing algorithm must include Time-To-Live (TTL) and packet deduplication to prevent network collapse.
3.  **App Distribution:** In a disaster scenario without internet, how do users download the app? Consider implementing a feature where the app can share its own installer (APK on Android) via a captive portal over Wi-Fi Direct.
4.  **MAC Address Privacy:** User proposal requested hashing the MAC address. As noted in section 2, OS level restrictions make static MAC address retrieval difficult. Using an App-generated UUID on first install is functionally identical and more robust.
