# ReUnite Mobile — Flutter App Boilerplate

Feature-based Flutter mobile app structure for the **ReUnite Offline P2P Emergency Mesh**.

---

## Directory Structure & Developer Work-Split

```text
mobile/
├── pubspec.yaml                 # Dependencies (flutter_blue_plus, provider, geolocator)
└── lib/
    ├── main.dart                # App entry point
    ├── app.dart                 # Navigation & Tab Bar
    ├── services/
    │   └── mesh_service.dart    # Shared Mesh Core state & Rust bridge interface
    ├── features/
    │   ├── chat/                # DEVELOPER 1: Chat screens & messaging UI
    │   │   ├── chat_screen.dart
    │   │   └── widgets/
    │   └── map/                 # DEVELOPER 2: GPS Radar & Peer Distance Map
    │       └── map_screen.dart
    └── shared/
        └── theme.dart           # Dark Emergency Mode UI theme
```

---

## How to Run

1. **Install Flutter**: Make sure Flutter SDK is installed on your Mac or PC (`flutter --version`).
2. **Get Dependencies**:
   ```bash
   cd mobile
   flutter pub get
   ```
3. **Launch Mobile App**:
   ```bash
   flutter run
   ```

---

## Work Split Guidelines

* **Developer 1 (Chat & Mesh Bridge)**: Work inside `lib/features/chat/` & `lib/services/mesh_service.dart`.
* **Developer 2 (GPS Map & Networks)**: Work inside `lib/features/map/` & `lib/features/networks/`.
