//! `date_sun_info($timestamp, $latitude, $longitude)` returns daylight and twilight times.
//! The orbital model follows timelib's use of Paul Schlyter's public-domain algorithm.

use std::f64::consts::PI;

use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Value, args};

use crate::{calendar::Civil, normalize, zone};

const RADIANS: f64 = PI / 180.0;
const DEGREES: f64 = 180.0 / PI;
const REVOLUTION: f64 = 1.0 / 360.0;

pub fn info(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let timestamp = args::int(arguments, 0).expect("timestamp");
    let latitude = args::float(arguments, 1).expect("latitude");
    let longitude = args::float(arguments, 2).expect("longitude");
    for (index, name, value) in [(2, "latitude", latitude), (3, "longitude", longitude)] {
        if !value.is_finite() {
            return Err(NativeError::value_error(format!(
                "date_sun_info(): Argument #{index} (${name}) must be finite"
            )));
        }
    }
    let (date, _) = zone::current(context).local(timestamp);
    let midnight = normalize::timestamp(Civil {
        hour: 0,
        minute: 0,
        second: 0,
        ..date
    });
    let solar = solar_position(midnight, longitude);
    let horizon = -35.0 / 60.0 - 0.2666 / solar.distance;
    let mut result = PhpArray::new();
    for (above, below, altitude) in [
        ("sunrise", "sunset", horizon),
        ("civil_twilight_begin", "civil_twilight_end", -6.0),
        ("nautical_twilight_begin", "nautical_twilight_end", -12.0),
        (
            "astronomical_twilight_begin",
            "astronomical_twilight_end",
            -18.0,
        ),
    ] {
        let (rise, set) = crossing(midnight, &solar, latitude, altitude);
        result.insert(ArrayKey::str(above), rise);
        result.insert(ArrayKey::str(below), set);
        if above == "sunrise" {
            result.insert(
                ArrayKey::str("transit"),
                seconds(midnight as f64 + solar.noon * 3600.0),
            );
        }
    }
    Ok(result.into())
}

struct SolarPosition {
    noon: f64,
    declination: f64,
    distance: f64,
}

fn solar_position(midnight: i64, longitude: f64) -> SolarPosition {
    let julian = midnight as f64 / 86_400.0 + 2_440_587.5;
    let day = (julian - 2_451_545.0) + 2.0 - longitude / 360.0;
    let mean_longitude = (180.0 + 356.0470 + 282.9404) + (0.9856002585 + 4.70935e-5) * day;
    let sidereal = revolution(revolution(mean_longitude) + 180.0 + longitude);

    let anomaly = revolution(356.0470 + 0.9856002585 * day);
    let perihelion = 282.9404 + 4.70935e-5 * day;
    let eccentricity = 0.016709 - 1.151e-9 * day;
    let eccentric_anomaly =
        anomaly + eccentricity * DEGREES * sin(anomaly) * (1.0 + eccentricity * cos(anomaly));
    let orbital_x = cos(eccentric_anomaly) - eccentricity;
    let orbital_y = (1.0 - eccentricity * eccentricity).sqrt() * sin(eccentric_anomaly);
    let distance = (orbital_x * orbital_x + orbital_y * orbital_y).sqrt();
    let mut sun_longitude = DEGREES * orbital_y.atan2(orbital_x) + perihelion;
    if sun_longitude >= 360.0 {
        sun_longitude -= 360.0;
    }

    let x = distance * cos(sun_longitude);
    let ecliptic_y = distance * sin(sun_longitude);
    let obliquity = 23.4393 - 3.563e-7 * day;
    let z = ecliptic_y * sin(obliquity);
    let y = ecliptic_y * cos(obliquity);
    let ascension = DEGREES * y.atan2(x);
    let declination = DEGREES * z.atan2((x * x + y * y).sqrt());
    let angle = sidereal - ascension;
    let opposite = angle - 360.0 * (angle * REVOLUTION + 0.5).floor();
    SolarPosition {
        noon: 12.0 - opposite / 15.0,
        declination,
        distance,
    }
}

fn crossing(midnight: i64, sun: &SolarPosition, latitude: f64, altitude: f64) -> (Value, Value) {
    let hour_cosine = (sin(altitude) - sin(latitude) * sin(sun.declination))
        / (cos(latitude) * cos(sun.declination));
    if hour_cosine >= 1.0 {
        (Value::Bool(false), Value::Bool(false))
    } else if hour_cosine <= -1.0 {
        (Value::Bool(true), Value::Bool(true))
    } else {
        let arc = DEGREES * hour_cosine.acos() / 15.0;
        (
            seconds((sun.noon - arc) * 3600.0 + midnight as f64),
            seconds((sun.noon + arc) * 3600.0 + midnight as f64),
        )
    }
}

fn revolution(degrees: f64) -> f64 {
    degrees - 360.0 * (degrees * REVOLUTION).floor()
}

fn sin(degrees: f64) -> f64 {
    (degrees * RADIANS).sin()
}

fn cos(degrees: f64) -> f64 {
    (degrees * RADIANS).cos()
}

fn seconds(value: f64) -> Value {
    // C conversions of NaN and out-of-range values follow the target CPU.
    let value = if !cfg!(target_arch = "aarch64")
        && !(i64::MIN as f64..-(i64::MIN as f64)).contains(&value)
    {
        i64::MIN
    } else {
        value as i64
    };
    Value::Int(value)
}
