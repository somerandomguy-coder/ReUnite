//! Adaptive beacon and scan cadence (phase 2D).
//!
//! A node alone in a field was beaconing every 3 seconds and scanning at the radio's
//! lowest-latency setting forever. That is a flat battery by morning, spent on an empty
//! room - and a flat battery is a person who has left the mesh. It is also the single
//! biggest gap between this app and something anyone would actually leave running.
//!
//! So the cadence backs off the longer nobody answers, and snaps back the instant anyone
//! does. Two asymmetries drive every number here:
//!
//! * **Being slow to notice a rescuer costs more than a few extra beacons.** So the climb
//!   down is gradual and the climb back is immediate - one frame from anyone, even one we
//!   cannot decrypt, resets to the fastest rate.
//! * **An SOS is exactly the moment to spend the battery.** While one is active, ours or
//!   anyone's, there is no backing off at all.
//!
//! This module is deliberately pure: no clock, no radio, no randomness. It maps
//! observations to a cadence, which is what makes the ladder testable at all.

use core::time::Duration;

/// How hard the radio should be listening.
///
/// These map onto Android's `ScanSettings` modes directly. iOS has no equivalent knob -
/// CoreBluetooth chooses for itself - so there the *window* is what varies and the mode
/// is advisory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanMode {
    LowLatency,
    Balanced,
    LowPower,
}

impl ScanMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ScanMode::LowLatency => "low_latency",
            ScanMode::Balanced => "balanced",
            ScanMode::LowPower => "low_power",
        }
    }
}

/// What the radio should be doing right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cadence {
    /// How often to send a `Hello`.
    pub hello: Duration,
    pub scan: ScanMode,
    /// `Some((window, period))` means "scan for `window`, then sleep until `period` has
    /// passed, and repeat". `None` means scan continuously.
    ///
    /// Duty-cycling the *scan* is the half that actually saves the battery. Backing off
    /// only the beacon, which is the obvious thing to do, saves very little: a receiver
    /// listening at full tilt costs far more than a transmitter speaking occasionally.
    pub scan_window: Option<(Duration, Duration)>,
}

impl Cadence {
    /// The rate used whenever anyone is around, and for the first minute alone.
    pub const ENGAGED: Cadence = Cadence {
        hello: Duration::from_secs(3),
        scan: ScanMode::LowLatency,
        scan_window: None,
    };
}

/// Below this charge the node drops one rung further down the ladder.
///
/// This is what finally makes the battery byte in the beacon worth carrying: a node that
/// is nearly flat should still be findable in an hour, and it will not be if it spends
/// what is left talking to nobody.
pub const LOW_BATTERY_PERCENT: u8 = 15;

/// One rung of the ladder: "once you have been alone this long, behave like this".
const LADDER: &[(u64, Cadence)] = &[
    // Alone under a minute is the normal join race, not solitude. Do not back off into it.
    (
        60_000,
        Cadence {
            hello: Duration::from_secs(3),
            scan: ScanMode::LowLatency,
            scan_window: None,
        },
    ),
    // Probably alone, possibly not.
    (
        5 * 60_000,
        Cadence {
            hello: Duration::from_secs(10),
            scan: ScanMode::Balanced,
            scan_window: None,
        },
    ),
    // Alone. Stay findable, stop burning.
    (
        20 * 60_000,
        Cadence {
            hello: Duration::from_secs(30),
            scan: ScanMode::LowPower,
            scan_window: Some((Duration::from_secs(5), Duration::from_secs(30))),
        },
    ),
    // The overnight case.
    (
        u64::MAX,
        Cadence {
            hello: Duration::from_secs(60),
            scan: ScanMode::LowPower,
            scan_window: Some((Duration::from_secs(5), Duration::from_secs(60))),
        },
    ),
];

/// Everything the cadence depends on, gathered in one place so the decision is a function
/// rather than a scattering of conditions.
#[derive(Clone, Copy, Debug)]
pub struct Conditions {
    /// How long since we last heard **anything** from **anyone**.
    pub alone_for_ms: u64,
    /// Neighbours currently believed reachable.
    pub peers: usize,
    /// Any SOS active on this node or on a peer we can see.
    pub sos: bool,
    /// Battery charge, if this platform reports one.
    pub battery: Option<u8>,
}

/// The cadence to use under these conditions.
pub fn cadence(c: Conditions) -> Cadence {
    // Someone is there, or someone is in trouble. Neither is a moment to save power.
    if c.peers > 0 || c.sos {
        return Cadence::ENGAGED;
    }

    let mut rung = LADDER
        .iter()
        .position(|(limit, _)| c.alone_for_ms < *limit)
        .unwrap_or(LADDER.len() - 1);

    if matches!(c.battery, Some(b) if b < LOW_BATTERY_PERCENT) {
        rung = (rung + 1).min(LADDER.len() - 1);
    }

    LADDER[rung].1
}

/// Spread an interval by up to ±20 %.
///
/// Twenty phones that started together beacon in lockstep forever, colliding on air every
/// single time, and the collisions are worst exactly when the room is fullest. The jitter
/// is not a nicety; without it a crowd is quieter than a pair.
///
/// `seed` is supplied by the caller - usually a counter or a node id - so this stays a
/// pure function and the tests stay deterministic.
pub fn jitter(period: Duration, seed: u64) -> Duration {
    let millis = period.as_millis() as u64;
    if millis == 0 {
        return period;
    }
    // A cheap integer hash; the quality bar here is "not correlated between nodes", not
    // cryptographic. Anything heavier would be spending cycles to save cycles.
    let mixed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let spread = millis / 5; // 20 %
    if spread == 0 {
        return period;
    }
    let offset = (mixed >> 33) % (spread * 2 + 1);
    Duration::from_millis(millis + offset - spread)
}
