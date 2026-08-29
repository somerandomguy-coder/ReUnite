//! Beacon v1 - the BLE-advertisement-sized wire format (plan.md §2).
//!
//! `plan.md` requires that routing presence, SOS, battery, GPS and heat-map data ride
//! inside BLE advertising packets rather than paired connections. A legacy BLE
//! advertisement carries 31 bytes of AD payload; a manufacturer-specific AD structure
//! spends 2 bytes on length+type and 2 on the company id, leaving **27 usable bytes**.
//!
//! `Frame`/`Packet` (see `packet.rs`) are `bincode` and run to kilobytes, so they cannot
//! be advertised - they are for connection-oriented traffic (chat, invites). This module
//! is the other half: a hand-packed, fixed-layout codec that fits the budget.
//!
//! Deliberately `core`-only - no `std`, no `alloc`, no allocation anywhere. Encoding
//! writes into a caller-visible fixed buffer. This is one of the pieces that moves into
//! the `#![no_std]` core in Phase 3, and it is the format an ESP32 will emit directly.

use crate::types::NodeId;

/// Protocol version carried in the high nibble of byte 0.
pub const BEACON_VERSION: u8 = 1;
/// Usable manufacturer-specific data in a legacy BLE advertisement.
pub const MAX_BEACON_BYTES: usize = 27;
pub const HEADER_BYTES: usize = 4;
pub const PRESENCE_BYTES: usize = 23;
pub const ZONE_BYTES: usize = 22;

pub const TYPE_PRESENCE: u8 = 0;
pub const TYPE_ZONE: u8 = 1;

// ---- byte 1: flags ----
pub const FLAG_SOS: u8 = 1 << 0;
pub const FLAG_GPS: u8 = 1 << 1;
pub const FLAG_STATUS: u8 = 1 << 2;
pub const FLAG_RELAY: u8 = 1 << 3;

/// Battery percentage sentinel for "this platform will not tell us".
pub const BATTERY_UNKNOWN: u8 = 0xFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeaconError {
    TooShort,
    UnsupportedVersion(u8),
    UnknownType(u8),
}

impl core::fmt::Display for BeaconError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BeaconError::TooShort => write!(f, "beacon too short"),
            BeaconError::UnsupportedVersion(v) => {
                write!(f, "unsupported beacon version {v}, this build speaks {BEACON_VERSION}")
            }
            BeaconError::UnknownType(t) => write!(f, "unknown beacon type {t}"),
        }
    }
}

/// Fields shared by every beacon type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub flags: u8,
    /// 0..=100, or `BATTERY_UNKNOWN`.
    pub battery: u8,
    /// Wraps. Lets a receiver drop a repeat without a 128-bit packet id.
    pub seq: u8,
}

impl Header {
    pub fn sos(&self) -> bool {
        self.flags & FLAG_SOS != 0
    }
    pub fn has_gps(&self) -> bool {
        self.flags & FLAG_GPS != 0
    }
    pub fn has_status(&self) -> bool {
        self.flags & FLAG_STATUS != 0
    }
}

/// Type 0 - "I am here, this is my state".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Presence {
    pub node: NodeId,
    /// Latitude in degrees x 1e7. Meaningless unless `Header::has_gps`.
    pub lat_e7: i32,
    pub lon_e7: i32,
    /// Pre-canned status code, see `status.rs`.
    pub status: u8,
    pub hops: u8,
    pub ttl: u8,
}

/// Type 1 - "this hex cell is this safe, and this many of us agree".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zone {
    pub origin: NodeId,
    /// H3 cell index.
    pub cell: u64,
    /// Aggregated safety, 0 (dangerous) ..= 255 (safe).
    pub level: u8,
    /// Distinct reporters, saturating at 255. plan.md §3.2 "Trust Consensus".
    pub consensus: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Body {
    Presence(Presence),
    Zone(Zone),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Beacon {
    pub header: Header,
    pub body: Body,
}

/// A fixed-size encode buffer. `len` bytes of `bytes` are meaningful.
pub struct Encoded {
    pub bytes: [u8; MAX_BEACON_BYTES],
    pub len: usize,
}

impl Encoded {
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// Degrees -> the i32 fixed-point form used on the wire.
/// Latitude reaches 9.0e8 and longitude 1.8e9, both inside `i32`.
pub fn to_e7(degrees: f64) -> i32 {
    (degrees * 1e7).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

pub fn from_e7(value: i32) -> f64 {
    value as f64 / 1e7
}

impl Beacon {
    pub fn type_code(&self) -> u8 {
        match self.body {
            Body::Presence(_) => TYPE_PRESENCE,
            Body::Zone(_) => TYPE_ZONE,
        }
    }

    pub fn encode(&self) -> Encoded {
        let mut bytes = [0u8; MAX_BEACON_BYTES];
        bytes[0] = (BEACON_VERSION << 4) | (self.type_code() & 0x0f);
        bytes[1] = self.header.flags;
        bytes[2] = self.header.battery;
        bytes[3] = self.header.seq;
        let len = match &self.body {
            Body::Presence(p) => {
                bytes[4..12].copy_from_slice(&p.node.0);
                bytes[12..16].copy_from_slice(&p.lat_e7.to_le_bytes());
                bytes[16..20].copy_from_slice(&p.lon_e7.to_le_bytes());
                bytes[20] = p.status;
                bytes[21] = p.hops;
                bytes[22] = p.ttl;
                PRESENCE_BYTES
            }
            Body::Zone(z) => {
                bytes[4..12].copy_from_slice(&z.origin.0);
                bytes[12..20].copy_from_slice(&z.cell.to_le_bytes());
                bytes[20] = z.level;
                bytes[21] = z.consensus;
                ZONE_BYTES
            }
        };
        Encoded { bytes, len }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BeaconError> {
        if bytes.len() < HEADER_BYTES {
            return Err(BeaconError::TooShort);
        }
        let version = bytes[0] >> 4;
        if version != BEACON_VERSION {
            return Err(BeaconError::UnsupportedVersion(version));
        }
        let header = Header {
            flags: bytes[1],
            battery: bytes[2],
            seq: bytes[3],
        };
        let body = match bytes[0] & 0x0f {
            TYPE_PRESENCE => {
                if bytes.len() < PRESENCE_BYTES {
                    return Err(BeaconError::TooShort);
                }
                let mut node = [0u8; NodeId::LEN];
                node.copy_from_slice(&bytes[4..12]);
                Body::Presence(Presence {
                    node: NodeId(node),
                    lat_e7: i32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
                    lon_e7: i32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]),
                    status: bytes[20],
                    hops: bytes[21],
                    ttl: bytes[22],
                })
            }
            TYPE_ZONE => {
                if bytes.len() < ZONE_BYTES {
                    return Err(BeaconError::TooShort);
                }
                let mut origin = [0u8; NodeId::LEN];
                origin.copy_from_slice(&bytes[4..12]);
                let mut cell = [0u8; 8];
                cell.copy_from_slice(&bytes[12..20]);
                Body::Zone(Zone {
                    origin: NodeId(origin),
                    cell: u64::from_le_bytes(cell),
                    level: bytes[20],
                    consensus: bytes[21],
                })
            }
            other => return Err(BeaconError::UnknownType(other)),
        };
        Ok(Beacon { header, body })
    }
}
