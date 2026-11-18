//! Resolve local wall time: a spring-forward `02:30` normalizes to `03:30`.
//! During an overlap PHP consults the source date's DST flag when one exists.

use crate::{calendar::Civil, zone::Zone};

pub fn local_timestamp(zone: &Zone, civil: Civil, daylight: Option<bool>) -> i64 {
    let wall = crate::normalize::timestamp(civil);
    let preferred = zone.offset(wall);
    let guess = wall.wrapping_sub(i64::from(preferred.seconds));
    let mut candidates = Vec::new();
    for shift in [0, -172800, 172800] {
        let offset = zone.offset(guess.wrapping_add(shift));
        let timestamp = wall.wrapping_sub(i64::from(offset.seconds));
        let actual = zone.offset(timestamp);
        if actual.seconds == offset.seconds && !candidates.contains(&timestamp) {
            candidates.push(timestamp);
        }
    }
    if let Some(daylight) = daylight
        && let Some(timestamp) = candidates
            .iter()
            .find(|timestamp| zone.offset(**timestamp).daylight == daylight)
    {
        return *timestamp;
    }
    if let Some(timestamp) = candidates.first() {
        return *timestamp;
    }
    // A gap has no matching instant. Interpret it using the preceding offset,
    // preserving elapsed minutes instead of snapping to the transition.
    wall.wrapping_sub(i64::from(zone.offset(guess.wrapping_sub(172800)).seconds))
}
