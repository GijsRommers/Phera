//! At an overlap, `$date->modify('+1 day')` can retain the original side of the transition.

use super::transition_list;
use crate::{calendar::Civil, normalize, zone::Zone};

pub fn resolve(zone: &Zone, civil: Civil, daylight: Option<bool>) -> i64 {
    let wall = normalize::timestamp(civil);
    let current = zone.offset(wall);
    let current_offset = i64::from(current.seconds);
    let guess = wall.wrapping_sub(current_offset);
    let mut actual_offset = i64::from(zone.offset(guess).seconds);
    let mut transition = None;

    if current_offset == actual_offset
        && let Some(daylight) = daylight
    {
        let direction = if current_offset >= 0 && daylight && !current.daylight {
            -7200
        } else if current_offset <= 0 && current.daylight && !daylight {
            7200
        } else {
            return guess;
        };
        let probe = guess.wrapping_add(direction);
        let offset = i64::from(zone.offset(probe).seconds);
        if offset != actual_offset {
            let guess_transition = transition_list::latest(zone, guess);
            let probe_transition = transition_list::latest(zone, probe);
            let candidate = wall.wrapping_sub(offset);
            if (direction < 0 && candidate < guess_transition)
                || (direction > 0 && candidate >= probe_transition)
            {
                actual_offset = offset;
                transition = Some(probe_transition);
            }
        }
    }
    if current_offset == actual_offset {
        return guess;
    }
    let transition = transition.unwrap_or_else(|| transition_list::latest(zone, guess));
    let adjusted = wall.wrapping_sub(actual_offset);
    let in_transition = transition != i64::MIN
        && (transition.wrapping_add(current_offset - actual_offset)..transition)
            .contains(&adjusted);
    if in_transition { guess } else { adjusted }
}
