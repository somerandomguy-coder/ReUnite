//! Battery-level telemetry (plan.md §3.1: beacons carry Hashed ID, SOS flag and battery).
//!
//! Knowing a peer is at 4% changes what you do about them, and Phase 2 uses the same
//! number to duty-cycle its own advertising. Reading it is platform-specific and slow
//! enough (macOS shells out to `pmset`) that it must never sit in the packet path, so the
//! result is cached for a minute.
//!
//! Unlike the rest of the new Phase 1 modules this one is unavoidably `std` - it reads
//! files and spawns processes. It stays in the `std` shell at the Phase 3 split.

use std::sync::Mutex;
use std::sync::OnceLock;

use crate::types::now_ms;

const CACHE_MS: u64 = 60_000;

struct Cache {
    read_at_ms: u64,
    value: Option<u8>,
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(Cache {
            read_at_ms: 0,
            value: None,
        })
    })
}

/// Battery charge 0..=100, or `None` when the platform will not say (desktop with no
/// battery, unsupported OS, or a failed read). Cached for 60s.
pub fn read_percent() -> Option<u8> {
    let now = now_ms();
    if let Ok(mut c) = cache().lock() {
        if c.read_at_ms != 0 && now.saturating_sub(c.read_at_ms) < CACHE_MS {
            return c.value;
        }
        let fresh = read_uncached();
        c.read_at_ms = now;
        c.value = fresh;
        return fresh;
    }
    read_uncached()
}

fn read_uncached() -> Option<u8> {
    platform_read().map(|p| p.min(100))
}

#[cfg(target_os = "linux")]
fn platform_read() -> Option<u8> {
    let dir = std::fs::read_dir("/sys/class/power_supply").ok()?;
    for entry in dir.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("BAT") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(entry.path().join("capacity")) {
            if let Ok(pct) = text.trim().parse::<u8>() {
                return Some(pct);
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn platform_read() -> Option<u8> {
    // `pmset -g batt` prints e.g. "  -InternalBattery-0 (id=...)   87%; discharging; ..."
    let out = std::process::Command::new("pmset")
        .args(["-g", "batt"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let percent_at = text.find('%')?;
    let digits: String = text[..percent_at]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.chars().rev().collect::<String>().parse::<u8>().ok()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn platform_read() -> Option<u8> {
    None
}
