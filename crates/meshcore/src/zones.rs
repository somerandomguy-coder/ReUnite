//! Aggregated safe-zone heat map (plan.md §3.2, §4 step 1.5).
//!
//! Raw coordinates would crash the network - a hundred people in a street each shouting a
//! distinct lat/lon is a broadcast storm with no useful aggregate at the end of it. So a
//! safety report is snapped to an **H3 hex cell** and only the cell travels.
//!
//! Two rules give the number meaning:
//!
//! * **One report per node per cell**, latest wins. Without this a single node could
//!   shout a zone green fifty times and manufacture a consensus.
//! * **Consensus is reported separately from level.** `plan.md` §3.2 requires the UI show
//!   how many people verified a zone *before* it renders the red/green gradient, because
//!   "safe, 1 person says so" and "safe, 30 people say so" are not the same claim.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use h3o::{CellIndex, LatLng, Resolution};
use serde::{Deserialize, Serialize};

use crate::store::{read_json, write_json};
use crate::types::NodeId;

/// H3 resolution 8 is ~0.46 km^2 per cell (~460 m edge): roughly a town block, which is
/// the granularity a person can actually verify by looking around them.
pub const DEFAULT_RESOLUTION: u8 = 8;

/// Reports older than this stop counting. A safe zone six hours ago is not evidence now.
pub const ZONE_TTL_MS: u64 = 6 * 60 * 60 * 1000;

/// The 0..=4 scale a user types at the CLI.
pub const MAX_LEVEL: u8 = 4;

/// User scale (0 dangerous ..= 4 safe) -> the 0..=255 byte carried on the wire.
pub fn level_to_byte(level: u8) -> u8 {
    (level.min(MAX_LEVEL) as u16 * 255 / MAX_LEVEL as u16) as u8
}

/// Wire byte -> the user scale, fractional so an average of mixed reports is visible.
pub fn byte_to_level(byte: u8) -> f64 {
    byte as f64 * MAX_LEVEL as f64 / 255.0
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
    /// Wire-scale safety byte, 0 (dangerous) ..= 255 (safe).
    pub level: u8,
    pub ts_ms: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Zone {
    pub cell: u64,
    /// One entry per reporting node - that is what makes `consensus` trustworthy.
    pub reports: HashMap<NodeId, Report>,
}

impl Zone {
    /// Mean of every current report, on the wire scale.
    pub fn level(&self) -> u8 {
        if self.reports.is_empty() {
            return 0;
        }
        let sum: u32 = self.reports.values().map(|r| r.level as u32).sum();
        (sum / self.reports.len() as u32) as u8
    }

    /// How many distinct nodes verified this cell. Saturates at 255 for the wire.
    pub fn consensus(&self) -> u8 {
        self.reports.len().min(255) as u8
    }

    pub fn last_update_ms(&self) -> u64 {
        self.reports.values().map(|r| r.ts_ms).max().unwrap_or(0)
    }
}

/// A row for the CLI or a map overlay.
#[derive(Clone, Debug)]
pub struct ZoneView {
    pub cell: u64,
    pub lat: f64,
    pub lon: f64,
    pub level: u8,
    pub consensus: u8,
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
}

pub struct ZoneBook {
    home: PathBuf,
    zones: BTreeMap<u64, Zone>,
    res: u8,
}

impl ZoneBook {
    pub fn load(home: &Path, res: u8) -> Result<Self> {
        let file: ZonesFile = read_json(&home.join("zones.json"))?.unwrap_or_default();
        let mut zones = BTreeMap::new();
        for rec in file.zones {
            let cell = u64::from_str_radix(&rec.cell, 16)
                .map_err(|e| anyhow!("zones.json: bad cell {}: {e}", rec.cell))?;
            let mut reports = HashMap::new();
            for (node, report) in rec.reports {
                reports.insert(NodeId::from_hex(&node)?, report);
            }
            zones.insert(cell, Zone { cell, reports });
        }
        Ok(Self {
            home: home.to_path_buf(),
            zones,
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
                let mut reports: Vec<(String, Report)> = z
                    .reports
                    .iter()
                    .map(|(id, r)| (id.to_hex(), *r))
                    .collect();
                reports.sort_by(|a, b| a.0.cmp(&b.0));
                ZoneRecord {
                    cell: format!("{:x}", z.cell),
                    reports,
                }
            })
            .collect();
        write_json(&self.home.join("zones.json"), &ZonesFile { zones })
    }

    /// Record one node's report for one cell. Returns true when the aggregate changed,
    /// so the caller only gossips something new.
    pub fn record(&mut self, cell: u64, reporter: NodeId, level: u8, ts_ms: u64) -> bool {
        let zone = self.zones.entry(cell).or_insert_with(|| Zone {
            cell,
            reports: HashMap::new(),
        });
        if let Some(existing) = zone.reports.get(&reporter) {
            // A node re-reporting the same cell replaces its own earlier opinion; it never
            // adds to the consensus count.
            if existing.ts_ms >= ts_ms && existing.level == level {
                return false;
            }
        }
        let before = (zone.level(), zone.consensus());
        zone.reports.insert(reporter, Report { level, ts_ms });
        before != (zone.level(), zone.consensus())
    }

    pub fn get(&self, cell: u64) -> Option<&Zone> {
        self.zones.get(&cell)
    }

    /// Our own reports, as `(cell, level)`. These are what we re-gossip - a node must
    /// only ever republish its own opinion, never the aggregate, or the consensus count
    /// would compound as reports bounce around the mesh.
    pub fn mine(&self, me: &NodeId) -> Vec<(u64, u8)> {
        self.zones
            .values()
            .filter_map(|z| z.reports.get(me).map(|r| (z.cell, r.level)))
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
        dropped
    }

    /// Every cell we know, safest first, then best-attested, then freshest.
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
                    level: z.level(),
                    consensus: z.consensus(),
                    age_ms: now_ms.saturating_sub(z.last_update_ms()),
                    mine: z.reports.contains_key(me),
                })
            })
            .collect();
        out.sort_by(|a, b| {
            b.level
                .cmp(&a.level)
                .then(b.consensus.cmp(&a.consensus))
                .then(a.age_ms.cmp(&b.age_ms))
        });
        out
    }
}
