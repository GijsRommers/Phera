//! Compare sunrise, transit and three twilights across latitudes and civil dates.

mod support;

use phera_extension_api::Value;
use support::Matrix;

#[test]
fn solar_position_matches_php() {
    let mut matrix = Matrix::new();
    for zone in [
        "UTC",
        "Asia/Kathmandu",
        "America/St_Johns",
        "Pacific/Chatham",
    ] {
        matrix.case("date_default_timezone_set", vec![zone.into()]);
        for timestamp in [
            -2_208_988_800,
            -1_234_567_890,
            -1,
            0,
            1,
            946_684_800,
            1_711_929_600,
            1_718_928_000,
            1_735_257_600,
            4_103_045_999,
            i64::MIN,
            i64::MAX,
        ] {
            for latitude in [
                -100.0, -90.0, -78.0, -52.25, -0.0, 0.0, 39.0, 68.0, 90.0, 180.0,
            ] {
                for longitude in [-190.5, -90.75, 0.0, 4.35, 90.0, 179.75, 360.0] {
                    matrix.case(
                        "date_sun_info",
                        vec![
                            Value::Int(timestamp),
                            Value::Float(latitude),
                            Value::Float(longitude),
                        ],
                    );
                }
            }
        }
    }
    for value in [
        Value::Float(f64::NAN),
        Value::Float(f64::INFINITY),
        Value::Null,
        Value::from("23.5"),
        Value::from("x"),
        Value::Bool(false),
    ] {
        matrix.case(
            "date_sun_info",
            vec![Value::Int(0), value.clone(), Value::Float(0.0)],
        );
        matrix.case(
            "date_sun_info",
            vec![Value::Int(0), Value::Float(0.0), value],
        );
    }
    matrix.finish("date-sun-info");
}
