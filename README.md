# 📡 ReUnite — Local-First P2P BLE Mesh & Disaster Emergency System

> **Offline-first, zero-infrastructure peer-to-peer messaging and emergency SOS mesh network operating entirely over Bluetooth Low Energy (BLE) radio.**

---

## 🛠️ Prerequisites & Environment Setup

Assume you are starting from scratch on a clean machine without any prior setup. Follow these quick steps to get your environment ready:

### 1. Install Node.js (v18+)
Verify if Node.js is installed:
```bash
node -v
npm -v
```
If not installed:
- **macOS / Linux**: `brew install node` or download from [nodejs.org](https://nodejs.org).
- **Ubuntu/Debian**: `sudo apt update && sudo apt install nodejs npm`

### 2. Install `uv` (Fast Python Package & Tool Runner)
[`uv`](https://github.com/astral-sh/uv) is an extremely fast Python package runner. It eliminates manual `venv` creation and `pip install` commands by managing isolated environments automatically when running scripts.

- **macOS / Linux**:
  ```bash
  curl -LsSf https://astral.sh/uv/install.sh | sh
  ```
- **Windows (PowerShell)**:
  ```powershell
  powershell -c "irm https://astral.sh/uv/install.ps1 | iex"
  ```
- **Via Homebrew**: `brew install uv`
- **Via Pip**: `pip install uv`

Verify installation:
```bash
uv --version
```

---

## 🏗️ Architecture: Local-First In-App "APIs"

In a local-first mobile architecture with no internet backend, you still have "APIs," but instead of HTTP REST endpoints (`POST /message`), your APIs are **TypeScript Interfaces and In-Memory Event Emitters**.

When everything runs on-device, team members decouple by agreeing on **Shared Contracts** (`src/contracts/mesh.ts`) and using **Mock Providers**. 

```
[ FRONTEND UI ] (Chat, Radar Map, SOS Panic Button)
      │
      ▼  API 1: Reactive State Hooks (useMeshStore, sendEmergencySOS)
[ LOCAL DATA & STATE STORE ] (Zustand / SQLite / StorageRepository)
      │
      ▼  API 2: Mesh Router Contract (sendMessage, onMessageReceived)
[ MESH ROUTING ENGINE ] (A -> B -> C Relay, TTL Hop Counter, Deduplication Cache)
      │
      ▼  API 3: Radio Driver Contract (broadcastPayload, onPayloadDiscovered)
[ HARDWARE BLE DRIVER ] (Connectionless Extended Advertising / Beacon Broadcast)
```

---

## 👥 Work Breakdown for 4 Developers (Parallel Workflow)

| Developer | Core Domain | How They Work Independently | Deliverables |
| :--- | :--- | :--- | :--- |
| **Dev 1 (Frontend)** | UI, Radar Map & SOS UX | Uses `MockMeshRouter` which emits simulated incoming emergency messages & radar blips on timers. Never touches BLE hardware. | • Chat & Broadcast UI<br>• SOS Panic Trigger UI<br>• Compass / Radar Map Node visualizer |
| **Dev 2 (Mesh Protocol)** | Multi-Hop Routing (`A -> B -> C -> D`) | Writes 100% pure TypeScript logic tested in Vitest/Jest. Simulates virtual nodes in memory talking via byte arrays. | • `PacketCodec.ts` (JSON ↔ Compact Uint8Array)<br>• `MeshRouter.ts` (Deduplication cache & TTL hop decrement) |
| **Dev 3 (Hardware / BLE)** | Multi-Peer BLE Driver | Works with native BLE code (CoreBluetooth / Android BLE). Upgrades 1-to-1 pairing to connectionless beacon advertising. | • `BleRadioDriver.ts`<br>• Continuous scanning & Extended Advertising beacons |
| **Dev 4 (Storage & State)** | Local Persistence & Store | Bridges Dev 2's protocol engine to Dev 1's UI state and manages deduplication. | • `StorageRepository.ts`<br>• `useMeshStore.ts` (Reactive UI store wrapper) |

---

## 🚀 Step-by-Step Execution Guide

### Step 1: Install Node Dependencies
```bash
npm install
```

### Step 2: Run Multi-Hop Virtual Relay Test (No Phones Needed!)
Test Node A → Node B → Node C relaying an emergency SOS and dropping duplicate packets in memory:

```bash
npm test
```

Outputs:
```text
✓ test/relay.test.ts (1 test)
  ✓ Multi-Hop Mesh Relay Engine (A -> B -> C)
    ✓ Node B should relay Node A emergency SOS to Node C and drop duplicate packets
```

---

### Step 3: Run Python BLE Chat & SOS Server (Using `uv`)
Run the Linux BLE Server Node directly with `uv` (it automatically creates a sandbox, installs dependencies from `pyproject.toml`, and runs the script):

```bash
uv run ble_chat_node.py
```

Outputs:
```text
=====================================================
📶 Starting Linux P2P BLE Chat & SOS Server...
=====================================================
✅ BLE GATT Service Registered!
   Service UUID: a1b2c3d4-e5f6-7890-1234-56789abcdef0
📡 Advertising as 'BitChat-Linux' over Bluetooth radio...
-----------------------------------------------------
Linux Terminal Chat >
```

> **Note (Running without `uv`)**: If you do not have `uv` installed, you can use traditional `venv` & `pip`:
> ```bash
> python3 -m venv .venv
> source .venv/bin/activate  # On Windows: .venv\Scripts\activate
> pip install bleak bluez-peripheral dbus-fast
> python3 ble_chat_node.py
> ```

---

### Step 4: Test Offline WebBluetooth Node on Android Phone
1. Turn **Airplane Mode ON** on your Android Phone (Wi-Fi OFF, Cellular OFF, **Bluetooth ON**).
2. Open `ble_mesh.html` in Google Chrome on Android.
3. Tap **📡 Connect to Linux Node** → Select **BitChat-Linux**.
4. Tap **🚨 SEND SOS EMERGENCY** or send chat messages over pure Bluetooth LE radio!

---

## 📂 Repository Layout

```text
ReUnite/
├── pyproject.toml              # UV Python package runner configuration
├── ble_chat_node.py            # Linux P2P BLE Chat & SOS Server (`uv run ble_chat_node.py`)
├── ble_mesh.html               # Mobile WebBluetooth Client for Android Chrome (100% offline)
├── package.json                # TypeScript & Vitest configuration
├── tsconfig.json               # TypeScript compiler config
├── src/
│   ├── contracts/
│   │   └── mesh.ts             # LOCKED Shared Contracts (IMeshRouter, IRadioDriver, MeshMessage)
│   ├── mocks/
│   │   └── MockMeshRouter.ts   # Mock Mesh Router for UI Dev (Dev 1)
│   ├── protocol/
│   │   ├── PacketCodec.ts      # Micro-Frame Encoder/Decoder (Dev 2)
│   │   └── MeshRouter.ts       # Epidemic Flooding Engine & Hop Decrement (Dev 2)
│   ├── radio/
│   │   └── BleRadioDriver.ts   # Connectionless BLE Radio Driver Blueprint (Dev 3)
│   ├── storage/
│   │   └── StorageRepository.ts# Local Cache & Deduplication Repository (Dev 4)
│   └── store/
│       └── useMeshStore.ts     # Reactive Store Wrapper (Dev 4)
└── test/
    └── relay.test.ts           # Vitest Multi-Hop Relay Simulation Test
```

---

## ⚡ Transitioning from 1-to-1 Pairing to Multi-Hop Mesh

### The Limitation of GATT Pairing
Standard GATT pairing/connecting limits a mobile phone to ~3–7 active simultaneous connections. You cannot scale a multi-hop mesh network using manual GATT pairing.

### Connectionless Advertising (Beacon Broadcast)
To scale across $N$ nodes without connection bottlenecks:

1. **Node A** packs a 30-byte micro-frame into the **Manufacturer Specific Data** field of a BLE Advertising packet.
2. **Node A** advertises this payload for ~300–800 ms.
3. **Node B** (scanning continuously in background) catches Node A's advertisement, extracts the raw bytes, passes them to `MeshRouter`:
   - Checks `seenPacketCache` (if already seen, **drop packet** to prevent infinite loops).
   - Decrements `hopsRemaining` (TTL 5 → 4).
   - Re-advertises the packet to Node C and nearby nodes.
4. **Zero connection handshake latency** across unlimited nodes.
