//! Safe / unsafe zone reporting (plan.md §3.2, §4 step 1.5; redesigned in phase 2B).
//!
//! Raw coordinates would crash the network - a hundred people in a street each shouting a
//! distinct lat/lon is a broadcast storm with no useful aggregate at the end of it. So a
//! report is snapped to an **H3 hex cell** and only the cell travels, carrying two more
//! things with it: a **verdict** and a **radius**.
//!
//! ## Why a verdict and not a scale
//!
//! Phase 1 carried a 0..=4 safety level averaged across reporters. Three problems, all of
//! them found by reading the question out loud:
//!
//! * **Nobody can answer it under stress.** "Is this a 2 or a 3?" has no defensible answer
//!   at 3 a.m. in a flooded street. "Is it safe here - yes or no?" does.
//! * **The mean blurs disagreement into a lie.** Two people reporting 4 and two reporting 0
//!   averaged to "2 - moderate", a sentence nobody said, painting a contested street amber.
//! * **The reported area was not the reporter's to choose.** One fixed cell, whether the
//!   person could vouch for their doorway or for the whole district.
//!
//! So: one bit of verdict, one radius the reporter picks, and disagreement stays visible.
//!
//! Two rules give the numbers meaning, and both survive from Phase 1 unchanged:
//!
//! * **One report per node per cell**, latest wins. Without this a single node could shout
//!   a zone safe fifty times and manufacture a consensus.
//! * **The two vote counts are reported separately**, never folded into one number.
//!   plan.md §3.2 requires the UI show how many people verified a zone; "5 say safe" and
//!   "5 say safe, 4 say unsafe" must not render identically.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use h3o::{CellIndex, LatLng, Resolution};
use serde::{Deserialize, Serialize};

use crate::store::{read_json, write_json};
use crate::types::NodeId;

/// H3 resolution 8 is ~0.46 km^2 per cell (~460 m edge): roughly a town block. The
/// reported radius is carried alongside and is what the map actually draws; the cell is
/// what keeps a position off the air.
pub const DEFAULT_RESOLUTION: u8 = 8;

/// Reports older than this stop counting. A safe zone six hours ago is not evidence now.
pub const ZONE_TTL_MS: u64 = 6 * 60 * 60 * 1000;

/// Below this the H3 cell is the real resolution anyway, so a smaller number would be a
/// precision the format cannot keep.
pub const MIN_RADIUS_M: u32 = 10;

/// Past this nobody is vouching for somewhere they have seen.
pub const MAX_RADIUS_M: u32 = 20_000;

/// How many of a node's **own** reports are kept for re-gossip.
///
/// Every node republishes its own reports indefinitely - Phase 1 found that a one-shot
/// broadcast is lost to the startup race and invisible to anyone who joins later. Without
/// a bound, a node that has walked across a city re-gossips a hundred cells forever. The
/// bound is also what makes this structure portable to Phase 3: on an nRF52 it becomes a
/// `heapless::Vec<_, 16>`, a compile-time constant rather than an allocation.
///
/// Sixteen also roughly matches how the information decays: with a six-hour TTL, a report
/// older than your last sixteen has usually expired already.
pub const OWN_REPORT_CAPACITY: usize = 16;

/// One bit: is it safe here, or is it not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    Safe,
    Unsafe,
}

pub const WIRE_UNSAFE: u8 = 0;
pub const WIRE_SAFE: u8 = 1;

impl Verdict {
    pub fn to_wire(self) -> u8 {
        match self {
            Verdict::Safe => WIRE_SAFE,
            Verdict::Unsafe => WIRE_UNSAFE,
        }
    }

    /// Anything that is not an explicit "safe" is treated as unsafe.
    ///
    /// This is the fail-safe direction on purpose. A corrupted or future-versioned byte
    /// that renders a street green is a worse outcome than one that renders it red, so an
    /// unrecognised value must never be able to clear a hazard.
    pub fn from_wire(byte: u8) -> Self {
        if byte == WIRE_SAFE {
            Verdict::Safe
        } else {
            Verdict::Unsafe
        }
    }

    pub fn is_safe(self) -> bool {
        matches!(self, Verdict::Safe)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Safe => "safe",
            Verdict::Unsafe => "unsafe",
        }
    }
}

/// Parse the word a person types at the CLI.
pub fn parse_verdict(word: &str) -> Option<Verdict> {
    match word.to_ascii_lowercase().as_str() {
        "safe" | "s" | "ok" | "green" => Some(Verdict::Safe),
        "unsafe" | "u" | "danger" | "dangerous" | "hazard" | "red" => Some(Verdict::Unsafe),
        _ => None,
    }
}

/// Convert a length the user typed into metres.
///
/// Feet and miles are supported because the people who need this app are not all on the
/// metric system, and a misread unit is a wrong search area.
pub fn to_metres(length: f64, unit: &str) -> Result<u32> {
    if !length.is_finite() || length <= 0.0 {
        return Err(anyhow!("radius must be a positive number"));
    }
    let metres = match unit.to_ascii_lowercase().as_str() {
        "m" | "metre" | "metres" | "meter" | "meters" => length,
        "km" | "kilometre" | "kilometres" | "kilometer" | "kilometers" => length * 1000.0,
        "ft" | "foot" | "feet" => length * 0.3048,
        "mi" | "mile" | "miles" => length * 1609.344,
        other => return Err(anyhow!("unknown unit '{other}' - use m, km, ft or mi")),
    };
    let rounded = metres.round();
    if rounded < MIN_RADIUS_M as f64 {
        return Err(anyhow!(
            "radius must be at least {MIN_RADIUS_M} m - below that the hex cell is the real resolution"
        ));
    }
    if rounded > MAX_RADIUS_M as f64 {
        return Err(anyhow!(
            "radius must be at most {} km - past that nobody is vouching for what they have seen",
            MAX_RADIUS_M / 1000
        ));
    }
    Ok(rounded as u32)
}

/// Human-readable radius, for a CLI table or a UI label.
pub fn fmt_radius(radius_m: u32) -> String {
    if radius_m >= 1000 {
        format!("{:.1} km", radius_m as f64 / 1000.0)
    } else {
        format!("{radius_m} m")
    }
}

/// Clamp a radius arriving off the wire. A peer on a different build must not be able to
/// make us draw a circle over a continent.
pub fn clamp_radius(radius_m: u32) -> u32 {
    radius_m.clamp(MIN_RADIUS_M, MAX_RADIUS_M)
}

pub fn resolution(res: u8) -> Result<Resolution> {
    Resolution::try_from(res).map_err(|e| anyhow!("invalid H3 resolution {res}: {e}"))
}

/// Snap a GPS fix to its H3 cell index.
pub fn cell_for(lat: f64, lon: f64, res: u8) -> Result<u64> {
    let ll = LatLng::new(lat, lon).map_err(|e| anyhow!("bad coordinates: {e}"))?;
    Ok(u64::from(ll.to_cell(resolution(res)?)))
}

/// The centre of a cell, for drawing it on a map or printing it in a table.
pub fn cell_center(cell: u64) -> Result<(f64, f64)> {
    let index = CellIndex::try_from(cell).map_err(|e| anyhow!("bad H3 cell: {e}"))?;
    let ll = LatLng::from(index);
    Ok((ll.lat(), ll.lng()))
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Report {
    pub verdict: Verdict,
    /// How far around themselves the reporter is vouching, in metres.
    pub radius_m: u32,
    pub ts_ms: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Zone {
    pub cell: u64,
    /// One entry per reporting node - that is what makes the vote counts trustworthy.
    pub reports: HashMap<NodeId, Report>,
}

impl Zone {
    pub fn safe_votes(&self) -> u32 {
        self.reports.values().filter(|r| r.verdict.is_safe()).count() as u32
    }

    pub fn unsafe_votes(&self) -> u32 {
        self.reports
            .values()
            .filter(|r| !r.verdict.is_safe())
            .count() as u32
    }

    /// The aggregate call for this cell.
    ///
    /// **A tie resolves to unsafe.** A contested area is not a safe area, and painting a
    /// street green because two people disagreed with two others is the failure mode that
    /// gets somebody hurt. The false alarms are worth it.
    pub fn verdict(&self) -> Verdict {
        if self.reports.is_empty() {
            return Verdict::Unsafe;
        }
        if self.safe_votes() > self.unsafe_votes() {
            Verdict::Safe
        } else {
            Verdict::Unsafe
        }
    }

    /// Mean radius of the reports that agree with the aggregate verdict.
    ///
    /// Averaging across *disagreeing* reporters would size the circle from people who were
    /// describing a different claim about the same ground.
    pub fn radius_m(&self) -> u32 {
        let verdict = self.verdict();
        let agreeing: Vec<u32> = self
            .reports
            .values()
            .filter(|r| r.verdict == verdict)
            .map(|r| r.radius_m)
            .collect();
        if agreeing.is_empty() {
            return MIN_RADIUS_M;
        }
        let sum: u64 = agreeing.iter().map(|r| *r as u64).sum();
        clamp_radius((sum / agreeing.len() as u64) as u32)
    }

    /// Everyone who has an opinion about this cell, saturating for the beacon's one byte.
    pub fn consensus(&self) -> u8 {
        self.reports.len().min(255) as u8
    }

    pub fn last_update_ms(&self) -> u64 {
        self.reports.values().map(|r| r.ts_ms).max().unwrap_or(0)
    }
}

/// A row for the CLI, the app's list, or a circle on the map.
#[derive(Clone, Debug)]
pub struct ZoneView {
    pub cell: u64,
    pub lat: f64,
    pub lon: f64,
    pub verdict: Verdict,
    pub radius_m: u32,
    pub safe_votes: u32,
    pub unsafe_votes: u32,
    pub age_ms: u64,
    /// True when this node contributed one of the reports.
    pub mine: bool,
}

#[derive(Serialize, Deserialize)]
struct ZoneRecord {
    cell: String,
    reports: Vec<(String, Report)>,
}

#[derive(Serialize, Deserialize, Default)]
struct ZonesFile {
    zones: Vec<ZoneRecord>,
    /// Cells this node has reported, oldest first. Persisted so the re-gossip ring
    /// survives a restart - otherwise a node reboots and starts republishing in a
    /// different order, or republishes something it had already aged out.
    #[serde(default)]
    own_ring: Vec<String>,
}

pub struct ZoneBook {
    home: PathBuf,
    zones: BTreeMap<u64, Zone>,
    /// Our own reports in report order, oldest at the front, capped at
    /// [`OWN_REPORT_CAPACITY`].
    own_ring: VecDeque<u64>,
    res: u8,
}

/// Move a file we cannot read aside and keep it.
///
/// Renamed rather than deleted, because it is the only evidence of what went wrong and a
/// destroyed file turns a bug report into a guess. A failed rename is ignored on purpose:
/// housekeeping must never be the reason a node does not start.
fn quarantine(path: &Path, why: &anyhow::Error) {
    let bad = path.with_extension("json.bad");
    let _ = std::fs::rename(path, &bad);
    eprintln!(
        "meshcore: {why:#} - moved to {} and continuing with an empty zone cache",
        bad.display()
    );
}

impl ZoneBook {
    /// Load the aggregated zone cache.
    ///
    /// **An unreadable cache degrades to an empty one; it never stops the node.** This
    /// file holds other people's safety votes, and every one of them is re-learned from
    /// the mesh within a gossip round or two. Refusing to start trades a recoverable
    /// cache for the entire node.
    ///
    /// That is not hypothetical. `df0bcbb` changed `Report` from `{level}` to
    /// `{verdict, radius_m}`, and every install that had ever written a `zones.json` came
    /// back up as *"the mesh core did not start"* - permanently, with no way out but
    /// deleting the app - on a phone whose owner may be trying to call for help.
    ///
    /// `identity.json` and `networks.json` deliberately keep failing loudly instead:
    /// losing an identity changes who you are to the mesh, and a lost network key cannot
    /// be recovered by any amount of gossip. Those are worth stopping for. This is not.
    pub fn load(home: &Path, res: u8) -> Result<Self> {
        let path = home.join("zones.json");
        let file: ZonesFile = match read_json(&path) {
            Ok(found) => found.unwrap_or_default(),
            Err(e) => {
                quarantine(&path, &e);
                ZonesFile::default()
            }
        };
        let mut zones = BTreeMap::new();
        for rec in file.zones {
            // One malformed record must not cost the other cells, for the same reason the
            // whole file must not cost the node. `own_ring` below has always skipped
            // rather than aborted; this brings the votes into line with it.
            let Ok(cell) = u64::from_str_radix(&rec.cell, 16) else {
                continue;
            };
            let mut reports = HashMap::new();
            for (node, report) in rec.reports {
                if let Ok(id) = NodeId::from_hex(&node) {
                    reports.insert(id, report);
                }
            }
            zones.insert(cell, Zone { cell, reports });
        }
        let mut own_ring = VecDeque::new();
        for cell in file.own_ring {
            if let Ok(cell) = u64::from_str_radix(&cell, 16) {
                own_ring.push_back(cell);
            }
        }
        while own_ring.len() > OWN_REPORT_CAPACITY {
            own_ring.pop_front();
        }
        Ok(Self {
            home: home.to_path_buf(),
            zones,
            own_ring,
            res,
        })
    }

    pub fn resolution(&self) -> u8 {
        self.res
    }

    pub fn save(&self) -> Result<()> {
        let zones = self
            .zones
            .values()
            .map(|z| {
                let mut reports: Vec<(String, Report)> =
                    z.reports.iter().map(|(id, r)| (id.to_hex(), *r)).collect();
                reports.sort_by(|a, b| a.0.cmp(&b.0));
                ZoneRecord {
                    cell: format!("{:x}", z.cell),
                    reports,
                }
            })
            .collect();
        let own_ring = self.own_ring.iter().map(|c| format!("{c:x}")).collect();
        write_json(&self.home.join("zones.json"), &ZonesFile { zones, own_ring })
    }

    /// Record one node's report for one cell. Returns true when the aggregate changed,
    /// so the caller only gossips something new.
    pub fn record(
        &mut self,
        cell: u64,
        reporter: NodeId,
        verdict: Verdict,
        radius_m: u32,
        ts_ms: u64,
    ) -> bool {
        let radius_m = clamp_radius(radius_m);
        let zone = self.zones.entry(cell).or_insert_with(|| Zone {
            cell,
            reports: HashMap::new(),
        });
        if let Some(existing) = zone.reports.get(&reporter) {
            // A node re-reporting the same cell replaces its own earlier opinion; it never
            // adds to either vote count.
            if existing.ts_ms >= ts_ms
                && existing.verdict == verdict
                && existing.radius_m == radius_m
            {
                return false;
            }
        }
        let before = (zone.verdict(), zone.safe_votes(), zone.unsafe_votes());
        zone.reports.insert(
            reporter,
            Report {
                verdict,
                radius_m,
                ts_ms,
            },
        );
        let after = (zone.verdict(), zone.safe_votes(), zone.unsafe_votes());
        before != after
    }

    /// Record *our own* report, which additionally moves the cell to the newest end of
    /// the re-gossip ring and evicts the oldest once past [`OWN_REPORT_CAPACITY`].
    ///
    /// Eviction only stops the cell being **republished**. The report itself stays in the
    /// aggregate until its TTL expires - dropping a vote we had already broadcast would
    /// silently withdraw a claim other nodes are still counting.
    pub fn record_own(
        &mut self,
        cell: u64,
        me: NodeId,
        verdict: Verdict,
        radius_m: u32,
        ts_ms: u64,
    ) -> bool {
        let changed = self.record(cell, me, verdict, radius_m, ts_ms);
        self.own_ring.retain(|c| *c != cell);
        self.own_ring.push_back(cell);
        while self.own_ring.len() > OWN_REPORT_CAPACITY {
            self.own_ring.pop_front();
        }
        changed
    }

    pub fn get(&self, cell: u64) -> Option<&Zone> {
        self.zones.get(&cell)
    }

    /// The cells we re-gossip, oldest first. A node must only ever republish **its own**
    /// opinion, never the aggregate, or the vote counts would compound as reports bounce
    /// around the mesh.
    pub fn mine(&self, me: &NodeId) -> Vec<(u64, Verdict, u32)> {
        self.own_ring
            .iter()
            .filter_map(|cell| {
                let report = self.zones.get(cell)?.reports.get(me)?;
                Some((*cell, report.verdict, report.radius_m))
            })
            .collect()
    }

    /// Drop reports past their TTL, then any cell left with nothing in it.
    pub fn prune(&mut self, now_ms: u64) -> usize {
        let mut dropped = 0;
        for zone in self.zones.values_mut() {
            let before = zone.reports.len();
            zone.reports
                .retain(|_, r| now_ms.saturating_sub(r.ts_ms) <= ZONE_TTL_MS);
            dropped += before - zone.reports.len();
        }
        self.zones.retain(|_, z| !z.reports.is_empty());
        let live: Vec<u64> = self.zones.keys().copied().collect();
        self.own_ring.retain(|c| live.contains(c));
        dropped
    }

    /// Every cell we know.
    ///
    /// **Unsafe first**, then best-attested, then freshest. Phase 1 sorted safest first,
    /// which is the wrong way round for a screen someone reads while deciding where to
    /// walk: the hazards are the rows that must not need scrolling to.
    pub fn views(&self, me: &NodeId, now_ms: u64) -> Vec<ZoneView> {
        let mut out: Vec<ZoneView> = self
            .zones
            .values()
            .filter_map(|z| {
                let (lat, lon) = cell_center(z.cell).ok()?;
                Some(ZoneView {
                    cell: z.cell,
                    lat,
                    lon,
                    verdict: z.verdict(),
                    radius_m: z.radius_m(),
                    safe_votes: z.safe_votes(),
                    unsafe_votes: z.unsafe_votes(),
                    age_ms: now_ms.saturating_sub(z.last_update_ms()),
                    mine: z.reports.contains_key(me),
                })
            })
            .collect();
        out.sort_by(|a, b| {
            a.verdict
                .is_safe()
                .cmp(&b.verdict.is_safe())
                .then((b.safe_votes + b.unsafe_votes).cmp(&(a.safe_votes + a.unsafe_votes)))
                .then(a.age_ms.cmp(&b.age_ms))
        });
        out
    }

    /// Export the zone map as a GeoJSON FeatureCollection.
    ///
    /// `radius_m` travels as a property so a consumer can draw the circle this describes;
    /// a bare point would lose the whole point of letting the reporter choose an area.
    pub fn to_geojson(&self, me: &NodeId, now_ms: u64) -> String {
        let features: Vec<String> = self
            .views(me, now_ms)
            .into_iter()
            .map(|v| {
                format!(
                    r#"{{"type":"Feature","geometry":{{"type":"Point","coordinates":[{},{}]}},"properties":{{"cell":"{:x}","status":"{}","radius_m":{},"safe_votes":{},"unsafe_votes":{},"mine":{},"age_ms":{}}}}}"#,
                    v.lon,
                    v.lat,
                    v.cell,
                    v.verdict.as_str(),
                    v.radius_m,
                    v.safe_votes,
                    v.unsafe_votes,
                    v.mine,
                    v.age_ms
                )
            })
            .collect();
        format!(
            r#"{{"type":"FeatureCollection","features":[{}]}}"#,
            features.join(",")
        )
    }
}
