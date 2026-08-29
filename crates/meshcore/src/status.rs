//! Pre-canned panic messages (plan.md §3.2, §4 step 1.2).
//!
//! A panicking person should not have to type, and BLE has no bandwidth for prose. So the
//! seven most useful things anyone needs to say in a disaster are each **one byte** on the
//! wire. The English text below exists only in the renderer - it never travels.
//!
//! This module is deliberately `core`-only (no `std`, no `alloc`): it is one of the pieces
//! that moves into the `#![no_std]` core in Phase 3.

/// Cleared / no status.
pub const NONE: u8 = 0x00;
pub const SAFE: u8 = 0x01;
pub const MEDICAL: u8 = 0x02;
pub const SUPPLIES: u8 = 0x03;
pub const TRAPPED: u8 = 0x04;
pub const MOVING: u8 = 0x05;
pub const SHELTER: u8 = 0x06;
pub const HAZARD: u8 = 0x07;

/// One row of the code table.
pub struct Status {
    pub code: u8,
    /// Short token accepted by `--status`.
    pub name: &'static str,
    /// What a human sees. Never transmitted.
    pub text: &'static str,
}

pub const TABLE: &[Status] = &[
    Status { code: SAFE,   name: "safe",   text: "🟢 Safe & Moving" },
    Status { code: HAZARD, name: "hazard", text: "⚠️ Hazard / Danger Spot" },
    Status { code: MEDICAL, name: "sos",   text: "🚨 SOS Emergency" },
];

pub fn lookup(code: u8) -> Option<&'static Status> {
    let mut i = 0;
    while i < TABLE.len() {
        if TABLE[i].code == code {
            return Some(&TABLE[i]);
        }
        i += 1;
    }
    None
}

/// Human text for a code. Unknown codes render as a number rather than being dropped:
/// a newer build may know a status this one does not, and losing it silently would be worse.
pub fn describe(code: u8) -> &'static str {
    match lookup(code) {
        Some(s) => s.text,
        None if code == NONE => "status cleared",
        None => "unknown status",
    }
}

pub fn name(code: u8) -> Option<&'static str> {
    match lookup(code) {
        Some(s) => Some(s.name),
        None => None,
    }
}

/// Parse `--status` input: either a token (`medical`) or a number (`2`).
pub fn parse(input: &str) -> Option<u8> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.eq_ignore_ascii_case("none") || trimmed.eq_ignore_ascii_case("clear") {
        return Some(NONE);
    }
    let mut i = 0;
    while i < TABLE.len() {
        if TABLE[i].name.eq_ignore_ascii_case(trimmed) {
            return Some(TABLE[i].code);
        }
        i += 1;
    }
    trimmed.parse::<u8>().ok().filter(|c| *c == NONE || lookup(*c).is_some())
}
