# Phase 2B — Safe / unsafe zones with a user-entered radius

> Inserted between Phase 2 and Phase 3. Goal: replace the 0–4 safety *gradient* with a
> binary **safe / unsafe** verdict that a person reports about a **radius around
> themselves**, and draw the result as overlapping translucent circles that darken where
> reports agree.

**Entry condition:** Phase 2A accepted.

---

## What changes and why

Today a report is "how safe is this hex cell, on a scale of 0 to 4". Three things are
wrong with that in the field:

* **A five-point scale is a question nobody can answer under stress.** "Is this a 2 or a
  3?" has no defensible answer at 3 a.m. in a flooded street. "Is it safe here — yes or
  no?" does.
* **The reported area is not the reporter's to choose.** The cell is fixed at H3
  resolution 8 (~460 m across) whether the person can vouch for their doorway or for the
  whole district. The person knows which; the app never asks.
* **The mean of a gradient is meaningless across disagreeing reporters.** Two people
  saying "4" and two saying "0" averages to "2 — moderate", which is a sentence no one
  intended and which paints a contested street amber.

After this phase: **one verdict bit, one radius the reporter chooses, and disagreement
stays visible as disagreement.**

---

## 2B.1 — The core model

### The report

```rust
pub enum Verdict { Safe, Unsafe }        // one byte on the wire

pub struct Report {
    pub verdict: Verdict,
    /// Radius the reporter vouches for, in metres.
    pub radius_m: u32,
    pub ts_ms: u64,
}
```

**Built as `u32`, not `u16`.** `u16` was chosen in this document for the two bytes it
saves. In `NetPayload` those two bytes are lost in `bincode` framing anyway, and a `u32`
removes a cast at every boundary — CLI, FFI, Dart. Beacon v1, where the byte budget is
real, still packs it as `u16`: 24 of the 27 available bytes.

The position is still **snapped to an H3 cell before it leaves the device** — that rule is
from `plan.md` §3.2 and it is not negotiable, it is what stops a hundred people in one
street from becoming a hundred distinct coordinates on the air. What changes is that the
cell now travels with a radius attached.

- [x] `zones.rs`: `Verdict`, `Report { verdict, radius_m, ts_ms }`, and delete
      `level_to_byte` / `byte_to_level` / `MAX_LEVEL`.
- [x] Wire: `NetPayload::Zone { cell: u64, verdict: u8, radius_m: u32 }` in `packet.rs`.
      Frame `VERSION` bumped 3 → 4; `Frame::decode` already reports a version mismatch
      clearly, so a mixed-build demo fails loudly rather than silently misreading a cell.
- [x] `Verdict::from_wire` treats **anything that is not an explicit `safe` byte as
      unsafe**. A corrupted or future-versioned value must never be able to clear a
      hazard. (Not in the original plan; found while writing the decoder.)
- [x] Beacon v1's zone body carries `verdict` + `radius_m` in place of the level byte,
      24 bytes of the 27 available.

### The 16-report ring buffer

Each node keeps **its own 16 most recent reports**, newest first, oldest evicted.

- [x] `ZoneBook::mine` becomes a bounded ring of 16. Re-gossip walks it one entry per
      maintenance tick, as today.

Three reasons this bound is the right shape:

* **It bounds re-gossip.** Every node republishes its own reports forever (Phase 1 found
  that a one-shot broadcast is lost to the startup race). Unbounded own-history means a
  node that has walked across a city gossips a hundred cells forever.
* **It bounds memory for Phase 3.** `heapless::Vec<Report, 16>` on an nRF52 is a
  compile-time constant, not an allocation.
* **It matches how the information decays.** With `ZONE_TTL_MS` at six hours, a report
  older than your last sixteen is almost certainly expired anyway.

Reports *received from other nodes* are not part of this bound — they stay keyed per cell
per node and are pruned by TTL, exactly as now.

### Aggregation: count people, not reports

Per cell, per side:

```
safe_votes   = distinct nodes whose latest report for this cell says Safe
unsafe_votes = distinct nodes whose latest report for this cell says Unsafe
verdict      = Unsafe if unsafe_votes >= safe_votes else Safe
```

- [x] `Zone::safe_votes()`, `Zone::unsafe_votes()`, `Zone::verdict()`.
- [x] **Ties resolve to unsafe.** A contested area is not a safe area. Painting a street
      green because two people disagreed with two others is the failure mode that gets
      somebody hurt, and it is worth the false alarms.
- [x] Both counts are carried to the UI **separately**. `plan.md` §3.2's trust-consensus
      requirement survives the redesign intact: "5 say safe" and "5 say safe, 4 say
      unsafe" must not render identically.

## 2B.2 — Input: a length field and a unit field

The reporter types a number and picks a unit. No presets, no slider.

```
Is it safe where you are?

  [  ✅  SAFE  ]        [  ⛔  UNSAFE  ]

Covering a radius of
  ┌──────────┐  ┌──────────┐
  │   500    │  │  metres ▾│      metres / kilometres / feet / miles
  └──────────┘  └──────────┘

  → about 3 minutes' walk. Snapped to hex cell 8865b5662bfffff before sending.
```

- [x] Units: metres, kilometres, feet, miles. Normalised to metres in the UI layer; the
      core only ever sees metres. Feet and miles are there because the people who need
      this app are not all on the metric system and a wrong unit is a wrong rescue area.
- [x] Validate: 10 m minimum (below that the H3 cell is the real resolution anyway),
      20 km maximum (past that nobody is vouching for anything they have seen).
- [x] Remember the last used unit. Nobody changes units twice. **Session-scoped**, held on
      `MeshService` so it survives tab changes and rebuilds; persisting it across launches
      needs a preferences store the app does not carry yet, and is not worth adding one for.
- [x] Show what is about to be claimed under the field, before it is claimed, including
      that the position is snapped to a hex cell first. The **cell id itself is not shown**
      as the mock suggested: it is computed in Rust from a GPS fix the UI has not taken
      yet at the moment of typing, and showing a stale or guessed one would be worse than
      showing none.
- [x] CLI parity: `--report-zone [lat] [lon] safe|unsafe [length] [unit]`, e.g.
      `--report-zone 10.7769 106.7009 unsafe 500 m`. Unit optional, defaults to metres.

## 2B.3 — Display: translucent circles that darken where they overlap

- [x] On the map, each aggregated cell draws a **circle** at the cell centre with the
      mean reported radius: green `#22C55E` for safe, red `#EF4444` for unsafe.
- [x] Base opacity **0.18 per layer**, composited normally, so two overlapping circles
      read as ~0.33 and five as ~0.63. Overlap density *is* the consensus signal — the
      darker the patch, the more people vouched for it.
- [x] Cap the composited alpha at **0.75**. A fully opaque overlay hides the map
      underneath it, and the map underneath it is how somebody navigates out.
- [x] Where a safe circle and an unsafe circle overlap, the unsafe one draws **on top**,
      with a 2 px solid stroke. Same reason ties resolve to unsafe: contested ground must
      not look settled.
- [x] Circles are drawn in `flutter_map`'s `CircleLayer` in metres (`useRadiusInMeter`),
      so they stay geographically true through zoom rather than being a fixed pixel blob.
- [x] Compass/Grid mode — still the no-tiles fallback — renders the same information as
      concentric range rings with a safe/unsafe colour and the vote counts, because a
      phone in a disaster usually has no tiles.
- [x] Legend: a swatch showing 1 / 3 / 5+ overlapping reports against their opacity.
      An opacity gradient with no key is decoration, not data.

## 2B.4 — Everything that reads the old model

- [x] `crates/meshffi/src/dto.rs`: `ZoneDto` loses `level` / `level_scaled`, gains
      `verdict`, `radius_m`, `safe_votes`, `unsafe_votes`.
- [x] `EventDto::ZoneUpdate` likewise.
- [x] `mobile/lib/models/mesh_models.dart`: `Zone` follows.
- [x] `ZoneBook::to_geojson`: `properties.status` becomes `"safe"` / `"unsafe"`, plus
      `radius_m`, `safe_votes`, `unsafe_votes`. Drop the `level < 200` → `"caution"`
      bucket — there is no caution any more, and a third value in a two-valued field is
      how consumers of the export get confused.
- [x] `crates/meshcli/src/render.rs`: `--heatmap show` columns become
      `CELL LAT LON VERDICT RADIUS SAFE UNSAFE AGE MINE`.
- [x] **`MeshService._autoLogSafePlace` must go.** It currently calls
      `reportZone(lat, lon, 4)` — "this place is maximally safe" — automatically every two
      minutes from a GPS fix, with no human involved. Under a binary model that is a
      machine casting a safety vote about a place nobody looked at, and it is the single
      most dangerous line in the app: it manufactures exactly the false consensus the
      trust-count exists to prevent. Auto-GPS position sharing stays; the automatic
      *verdict* goes. A safety claim needs a person behind it.

---

## Acceptance criteria

1. Reporting `safe` with `500 m` from the CLI and from the app produces the same cell,
   the same radius and the same aggregate on a third node.
2. Three nodes reporting one cell safe and three reporting it unsafe render **red**, and
   show `safe 3 / unsafe 3` — not a blended amber.
3. A node re-reporting the same cell replaces its own vote and does not change either
   count.
4. A node's 17th report evicts its 1st from the re-gossip ring, and the 1st stops being
   republished.
5. Five overlapping safe circles are visibly darker than one, and no stack reaches full
   opacity.
6. Unit conversion round-trips: `1640 ft`, `0.5 km` and `500 m` all produce `radius_m`
   within 1 m of each other.
7. A relay outside the private network still cannot read a `Zone` payload — the Phase 1
   test survives the wire-format change.
8. No automatic, unattended safety verdict is emitted anywhere in the app.

## Outcome

All eight acceptance criteria met. `./scripts/check.sh` is green: **28 Rust tests**
(21 in `mesh.rs`, up from 18) and **16 Dart tests**, `flutter analyze` clean.

Verified live on the CLI, including the unit conversions and the refusals:

```
[default] > --report-zone 10.7769 106.7009 unsafe 750 m
[default] reported unsafe within 750 m of cell 8865b5662bfffff - now reads unsafe (0 safe / 1 unsafe)
[default] > --report-zone 10.8269 106.7509 safe 0.5 km
[default] reported safe within 500 m of cell 8865b56767fffff - now reads safe (1 safe / 0 unsafe)
[default] > --report-zone 21.0278 105.8342 safe 1500 ft
[default] reported safe within 457 m of cell 88415cb4e3fffff - now reads safe (1 safe / 0 unsafe)
[default] > --report-zone 10.7769 106.7009 unsafe 90 furlongs
! unknown unit 'furlongs' - use m, km, ft or mi
```

Two decisions came out of the work rather than the plan:

* **Out-of-range radii are refused at input, but clamped off the wire.** A person who
  types the wrong unit must be told, not quietly given a different area than they asked
  for. A *peer* sending an out-of-range radius is a different situation: dropping their
  report would lose a hazard over a formatting disagreement, so `clamp_radius` pins it to
  the allowed band instead.
* **Sorting inverted.** Phase 1 listed safest first. For a screen someone reads while
  deciding where to walk, the hazards are the rows that must not need scrolling to, so
  `views()` now sorts unsafe first, then best-attested, then freshest.

## Deviations accepted

| # | `plan.md` requirement | Reality after 2B |
| :--- | :--- | :--- |
| D7 | §3.2 "Nodes broadcast a single byte representing the safety average of their hex grid" | A verdict bit plus a 16-bit radius, 3 bytes. The extra two bytes buy the reporter's own judgement about how far their claim extends, which the single byte cannot express. Still nowhere near raw coordinates, still one packet per cell. |
| D8 | §3.2 "Red/Green gradient" | Two colours, not a gradient. The gradient implied a precision the input never had. Density of agreement is now what varies, and it varies by opacity. |
