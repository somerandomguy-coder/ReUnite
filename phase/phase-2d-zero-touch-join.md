# Phase 2D — Zero-touch join and adaptive discovery

> Inserted between Phase 2 and Phase 3. Goal: **turning the device on is the whole
> onboarding.** Every device — phone or laptop — joins the mesh on every radio it has,
> with no picker, no configuration and no second thought, and backs off its beacon rate
> when nobody is listening so the battery survives the night.

**Entry condition:** Phase 2A accepted, 2C's root cause fixed (there is no point
auto-starting a radio that cannot start).

---

## 2D.1 — Every radio at once, not a radio picker

Today `MeshService.init(transport:)` picks **one** transport, and changing it calls
`switchTransport`, which stops the node and restarts it. The Networks tab has a radio
picker. That is a configuration question asked of somebody in an emergency, and its
correct answer is always "all of them".

- [x] `meshcore`: a `MultiTransport` implementing `Transport` over a set of child
      transports — `send_broadcast` fans out to every child, `send_to` picks the child
      that owns the link, `recv` merges. Routing, crypto and the actor see one radio and
      do not change.
- [x] Start every transport the device actually has, and treat each one's failure as
      **non-fatal**. A phone with Bluetooth off must still mesh over Wi-Fi; a laptop with
      no BLE peripheral role must still mesh over UDP. One dead radio may never take down
      the node.
- [x] Delete the radio picker. Replace it with a **status** panel: which radios are up,
      how many peers each is reaching, and the reason any of them is down.
- [x] The same peer reachable on two radios is one peer. Dedupe is already the router's
      job by node id; verify it holds when two links deliver the same origin.

**Why the picker has to go, not just default well.** A picker states that the choice
matters and that the user is qualified to make it. Neither is true. The radios have
different range, different power cost and different failure modes, and the mesh is
strictly better on all of them at once.

## 2D.2 — Adaptive beacon and scan duty cycle

A node alone in a field currently advertises every 3 seconds and scans at
`SCAN_MODE_LOW_LATENCY` forever. That is a flat battery by morning, spent on an empty
room — and a flat battery is a person who has left the mesh.

The ladder, driven by "how long since we heard *anything* from *anyone*":

| Time alone | Hello interval | BLE scan | Rationale |
| :--- | :--- | :--- | :--- |
| peers present | 3 s | low latency | Phase 1's rate. Someone is there; be responsive. |
| 0 – 1 min | 3 s | low latency | Do not back off during the normal join race. |
| 1 – 5 min | 10 s | balanced | Probably alone, possibly not. |
| 5 – 20 min | 30 s | low power, 5 s window every 30 s | Alone. Stay findable, stop burning. |
| > 20 min | 60 s | low power, 5 s window every 60 s | Long haul. This is the overnight case. |

- [x] Implement in the node actor as a state machine over the existing `hello` interval,
      so the CLI and both phones inherit it from one place.
- [x] **Snap back to the fastest rate the instant any frame arrives from anyone**,
      including one we cannot decrypt. The cost of being slow to notice a rescuer is not
      symmetrical with the cost of a few extra beacons.
- [x] **Never back off while an SOS is active** — ours or anyone's we are relaying. An SOS
      is exactly the moment to spend the battery.
- [x] Add jitter of ±20 % to every interval. Twenty phones that started together will
      otherwise beacon in lockstep forever, colliding on air every time.
- [x] Duty-cycle **scanning together with advertising**. Scanning is the more expensive
      half and backing off only the beacon saves little.
- [x] Feed the battery byte into the ladder: below 15 % charge, drop one rung further.
      This is what finally makes the battery telemetry in the beacon worth carrying.
- [ ] **Measure. Not done — it needs a phone.** Target from `plan.md`: < 5 %/hour idle,
      reported for alone-and-backed-off, one peer, five peers. The ladder is built and
      tested as a function; whether it hits the target on real silicon is unmeasured, and
      the number must be published either way rather than the target being moved.
- [x] **iOS gets less out of this than Android, and the doc must say so.** CoreBluetooth
      has no scan-mode knob and no advertising-interval control. The only lever there is
      stopping and restarting the scan, so only the *window* applies. A drain measurement
      that assumes parity between the platforms will be confusing.

## 2D.3 — Laptops join by being switched on

Three different machines, three honest answers, all documented rather than implied:

- [x] **Linux** — native BLE peripheral *and* central via BlueZ (`transport/ble_linux.rs`)
      plus Wi-Fi UDP, both at once under 2D.1. Full mesh peer.
- [x] **macOS / Windows** — no portable BLE peripheral role exists (deviation D3), so a
      laptop cannot *be* a BLE beacon. It joins over **Wi-Fi UDP**, and reaches BLE-only
      phones through `scripts/ble_gateway.py`, which bridges the BLE radio to a local node
      over UDP. This is a real limit of the platforms, not a gap in the plan.
- [x] Autostart, so "turning it on" is literally true:
      * Linux: a `systemd --user` unit.
      * macOS: a `launchd` plist in `~/Library/LaunchAgents`.
      * Windows: a Startup shortcut.
      All three ship in `scripts/autostart/` with an install command, and all three run
      the same zero-config `meshnet` with no arguments.
- [x] The GUI app on macOS starts its node on launch with no dialog, exactly as the phones
      do.

## 2D.4 — First-run: nothing to answer

- [x] No transport question, no network question, no account. Permissions are requested
      **when the radio needs them**, with a sentence saying what breaks without them.
- [x] A denied Bluetooth permission degrades to Wi-Fi with a visible, dismissible banner —
      never a dead end and never a modal the user must resolve before the app works.
- [x] The Peers screen states plainly which radios are live and how many peers each has
      found, because "no peers yet" and "the radio never started" look identical to a user
      and are completely different problems.

## 2D.5 — Documentation

- [x] `docs/JOINING.md` — **for users.** One page, no jargon: switch the device on, open
      the app, this is what you should see, this is what each radio can and cannot reach,
      this is what to do when you see nothing. Written for someone in a shelter, not for
      someone with a terminal.
- [x] `docs/SETUP.md` and `docs/MOBILE.md` — **for developers.** Autostart install, the
      multi-transport architecture, the duty-cycle ladder and how to read the radio status
      panel when diagnosing.
- [x] `docs/ARCHITECTURE.md` — `MultiTransport` and the duty-cycle state machine.

---

## Outcome

`./scripts/check.sh` is green: **34 Rust tests** (up from 28) and **17 Dart tests**,
`flutter analyze` clean. `swiftc -typecheck` clean; the Android APK builds.

| Criterion | Status |
| :--- | :--- |
| 1. Two phones and a laptop form a mesh unconfigured | **needs hardware** |
| 2. Bluetooth off → still meshing over Wi-Fi, reason shown | **built and tested** (`a_node_meshes_over_whichever_of_its_radios_is_working`) |
| 3. Wi-Fi off → still meshing over Bluetooth | same test, same mechanism |
| 4. Alone 25 min → 60 s beacon; back to 3 s within one interval | **ladder tested as a function**; end-to-end timing needs a device |
| 5. An active SOS never backs off | **tested** |
| 6. Measured idle drain published | **not done** — needs a phone |
| 7. A rebooted laptop rejoins with autostart | `scripts/autostart/install.sh`, **not verified across a real reboot** |
| 8. `docs/JOINING.md` followable cold | written |

Two things came out of the work rather than the plan:

* **`--transport` became a hint, not a switch.** It is kept only so the CLI and the tests
  can pin one radio; the app passes `all` and never asks. Removing the flag outright would
  have cost the tests their ability to isolate a transport, which is how the multi-radio
  behaviour is tested at all.
* **The ladder scales from the configured base rate rather than replacing it.** The
  integration tests pin a 300 ms hello interval; a ladder that hard-coded 3 s would have
  silently overridden them, and the tests would have passed for the wrong reason.

## Acceptance criteria

1. Two phones and a laptop, all switched on with the app installed and never configured,
   form a mesh.
2. Turning Bluetooth off on one phone leaves it meshing over Wi-Fi, with the reason shown.
3. Turning Wi-Fi off leaves it meshing over Bluetooth.
4. A node alone for 25 minutes is measurably beaconing at 60 s, and returns to 3 s within
   one interval of a peer appearing.
5. A node with an active SOS never backs off, whatever its solitude.
6. Measured idle drain over one hour, alone and backed off, is reported — and if it misses
   < 5 %/hour, the number is published anyway rather than the target being quietly moved.
7. A laptop rebooted with autostart installed rejoins with no human action.
8. `docs/JOINING.md` is followable by someone who has not read any other file in the repo.

## Deviations accepted

| # | `plan.md` requirement | Reality after 2D |
| :--- | :--- | :--- |
| D9 | §2 "Zero-Config Onboarding: generate a UUID, grant permissions, instantly join" | Held, and extended to every radio at once. Permissions are requested lazily rather than up front, because an app that asks for Bluetooth before showing anything is an app people deny and then uninstall. |
| D3 | Laptops advertise over BLE | Unchanged and still true: macOS and Windows cannot. 2D documents the gateway path rather than pretending the limit is gone. |
