//! Distance helpers used to rank peers by proximity (proposal: "nearest peers first").

use crate::types::Gps;

const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// Great-circle distance in metres between two GPS fixes.
pub fn haversine_m(a: &Gps, b: &Gps) -> f64 {
    let (lat1, lon1) = (a.lat.to_radians(), a.lon.to_radians());
    let (lat2, lon2) = (b.lat.to_radians(), b.lon.to_radians());
    let dlat = lat2 - lat1;
    let dlon = lon2 - lon1;
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

/// Human friendly distance rendering.
pub fn format_distance(metres: f64) -> String {
    if metres < 1000.0 {
        format!("{:.0}m", metres)
    } else {
        format!("{:.2}km", metres / 1000.0)
    }
}
