# Phase 2A — Build and display integrity

> Inserted between Phase 2 and Phase 3. Goal: the app compiles, launches and renders on
> every target, `cargo test` and `flutter analyze` are green, and no document promises a
> feature the code does not have.

**Entry condition:** Phase 2 partially accepted (2.1 radio half and 2.5 still open).

This phase fixes no product behaviour. It exists because Phases 2B, 2C and 2D are all
untestable on a tree that does not build, and because a stale test is worse than no test —
it fails for a reason nobody trusts, and people learn to ignore the suite.

---

## Entry state

Measured on this machine before any work:

| Check | Result |
| :--- | :--- |
| `flutter analyze` | **2 errors**, 1 warning, 3 infos |
| `cargo test --workspace` | **22 passed, 1 failed** |
| `git status` | **53 tracked files deleted** in the working tree, unstaged |

### 2A.1 — The working tree had deleted its own build system

Fifty-three tracked files were deleted and not committed. They were not incidental:

| Deleted | Referenced by | Consequence |
| :--- | :--- | :--- |
| `mobile/lib/shared/theme.dart` | `lib/app.dart:5` | Dart does not compile at all |
| `android/settings.gradle.kts`, `build.gradle.kts`, `gradle.properties`, `gradle/wrapper/*` | the Gradle build itself | no Android build is possible |
| `android/.../res/values/styles.xml`, `values-night/styles.xml` | `AndroidManifest.xml` `@style/LaunchTheme`, `@style/NormalTheme` | resource linking fails |
| `android/.../res/mipmap-*/ic_launcher.png` | `AndroidManifest.xml` `@mipmap/ic_launcher` | resource linking fails |
| `ios/Runner/Base.lproj/LaunchScreen.storyboard`, `Main.storyboard` | `Info.plist` `UILaunchStoryboardName`, `UIMainStoryboardFile` | iOS launches to a black window |
| `ios/Runner/Runner-Bridging-Header.h`, `SceneDelegate.swift` | the Xcode target | iOS build fails |

- [x] Restore all 53 from `HEAD` (`git checkout -- mobile/`). Reversible: they are in the
      last commit, and anything genuinely unwanted can be deleted again deliberately.
- [x] Confirm `flutter analyze` drops from 2 errors to 0.

**Why this is recorded rather than quietly fixed.** A tree that deletes its own launch
storyboard and its Gradle wrapper looks exactly like a tree in the middle of a
regeneration. The next person to see 53 deletions needs to know they were an accident and
that restoring them was the decision, not guess at it a second time.

### 2A.2 — The status table drifted from three sources of truth

Commit `6eba821` cut `crates/meshcore/src/status.rs` `TABLE` from seven pre-canned codes
to three, and relabelled `MEDICAL` from "Need medical help" to "🚨 SOS Emergency". The
code is the newest and the change was deliberate. Two things did not follow it:

- `crates/meshffi/tests/bridge.rs:197` still asserts `len() == 7` and that row 1 reads
  `"Need medical help"` — **this is the failing test**.
- `README.md`, `docs/MOBILE.md` and `phase/phase-2-mobile.md` still promise seven
  one-tap panic buttons.

- [x] Update the `meshffi` test to assert the real table, and to assert it *structurally*
      (every row matches `meshcore::status::TABLE`; codes are unique) so a future edit to
      the table does not have to edit the test.
- [x] Reconcile the docs to three codes wherever seven are promised.
- [x] Leave the constants `SUPPLIES`, `TRAPPED`, `MOVING`, `SHELTER` in place. They are
      still valid wire codes an older or newer build may send, and `describe()` already
      renders an unknown code rather than dropping it. Removing them would turn a
      forward-compatible protocol into a lossy one.

> **Open question for the product owner, not for this phase.** Three buttons or seven is a
> UX decision made in a commit, not in a design note. 2A only makes the tree self-consistent
> with whatever the answer is; it does not re-litigate it.

### 2A.3 — Analyzer warnings

- [x] `lib/bridge/mesh_ffi.dart:61` — `_hasLoadedNative` is written and never read. It
      turned out to be hiding a diagnostic: see 2A.5.
- [x] Three `withOpacity` deprecations (`map_screen.dart:161`,
      `networks_screen.dart:234,236`) → `withValues(alpha:)`. Doing this now matters
      because 2B adds a lot of new alpha-blended drawing, and the new code should not be
      written against a deprecated call.

### 2A.4 — Make the checks run without being asked

- [x] `scripts/check.sh`: `cargo test --workspace`, `cargo build --release`,
      `flutter analyze --fatal-warnings`, `flutter test`. One command, non-zero exit on
      any failure. It also **rebuilds `libmeshffi` when it is missing**, which is not a
      convenience — see 2A.6.

### 2A.5 — A failed core load was reported as "empty reply from core"

`MeshFfi._open()` builds a precise failure message naming every path it searched and
telling the reader to run `scripts/build_ffi.sh`. `MeshFfi.instance` caught that
exception, threw the message away, and substituted `MeshFfi._mock()` — a stub whose every
call returns null. Those nulls decoded to `{'type': 'error', 'message': 'empty reply from
core'}`, and that is the sentence the user got: a description of the symptom, with the
diagnosis discarded one frame earlier.

- [x] Keep the load error in `MeshFfi.loadError`; expose `MeshFfi.nativeLoaded` and an
      instance-level `isStub`.
- [x] `MeshService.init` checks `_ffi.isStub` first and surfaces the real message, so the
      startup screen names the missing build step.

### 2A.6 — Three display failures the restored build then exposed

Only reachable once the tree compiled again. Each was live in the shipped app:

* **The macOS and desktop app could not start at all.** `MeshService.init` defaults to
  `MeshTransport.bluetooth`; `_requestBluetoothPermissions` returns an error string on any
  platform without a native BLE layer; `init` treated that as fatal. Every desktop user
  saw **"The mesh core did not start — Bluetooth mesh is only available on Android and
  iOS"**, which reads as a broken build and sends people to rebuild a core that was fine.
  A missing radio must degrade to the one that is present, not end the app.
  - [x] Fall back to Wi-Fi with a `radioNotice`, not a `startError`.
  - This is the narrow version of what Phase 2D generalises into "every radio at once".

* **Compass/Grid mode stopped being the default view.** Commit `6eba821` added an
  interactive OpenStreetMap tab and made it the **first** tab, demoting the compass to
  second. `plan.md` §4 step 2.2 makes graceful degradation a hard requirement, and the
  new default fetches tiles from `tile.openstreetmap.org` — so the landing view of an
  offline-first disaster app was a blank grey grid on any phone without internet.
  - [x] Compass/Grid restored as the first tab; the interactive map is the opt-in second.

* **The peers empty state instructed every user to turn on Bluetooth**, including desktop
  users meshing over Wi-Fi, for whom the instruction does nothing. Same failure mode as
  the iOS bug in [2C](phase-2c-ble-interop.md) §2C.1: asserting a cause the code has not
  checked.
  - [x] The radar view now names the radio actually in use, and the empty state states the
        fact ("No peers heard yet") instead of issuing an instruction.

### 2A.7 — A stale build artifact was hiding the drift

Two Dart tests — `the panic buttons come from the core` and `tapping a panic button really
sends one byte` — asserted `statusCodes.length == 7` and looked for the literal string
`'Need medical help'`. They passed. They passed because `target/release/libmeshffi.dylib`
on this machine was **built before the status table was cut to three** and nobody had
rebuilt it. `scripts/check.sh` rebuilds the core, and both tests failed immediately.

- [x] Both rewritten to derive their expectations from `mesh.statusCodes` — asserting that
      every code the core carries has a button with the core's own label, which is the
      thing the test was named for. A test that hard-codes a copy of the answer verifies a
      copy, not a connection.

**This is the real argument for 2A.4.** A green suite run against a stale binary is worse
than a red one: it is a green light for a claim nobody checked.

---

## Acceptance criteria

1. `git status` is clean of unexplained deletions.
2. `flutter analyze` reports **0 issues** at all severities.
3. `cargo test --workspace` is **fully green**, with no test asserting a value the code no
   longer produces.
4. `flutter test` is green.
5. No document promises a status code the core does not carry.
6. `scripts/check.sh` exists and passes.

## Outcome

All six acceptance criteria met.

| Check | Before | After |
| :--- | :--- | :--- |
| `cargo test --workspace` | 22 passed, **1 failed** | **25 passed, 0 failed** |
| `flutter analyze` | **2 errors**, 1 warning, 3 infos | **0 issues** |
| `flutter test` | 6 passed, **7 failed** | **13 passed, 0 failed** |
| deleted tracked files | 53 | 0 |

Two things came out of the work rather than the plan, and both are behaviour, not tidying:
the desktop app could not start at all (2A.6), and the offline-first map had stopped
defaulting to its offline view (2A.6). Neither was visible from a phase document; both
were visible within a minute of the tree compiling again.

## Deviations accepted

None. 2A.6's Wi-Fi fallback changes startup behaviour, but toward what
[`plan.md`](../plan.md) §2 already requires of zero-config onboarding — it removes a dead
end, it does not add a rule.
