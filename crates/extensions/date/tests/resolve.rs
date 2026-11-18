//! Fixed dates compare wall-time resolution through PHP's DateTime constructor.
use phera_ext_date::{calendar::Civil, resolve::local_timestamp, zone::Zone};
use phera_extension_api::testing::oracle;

#[test]
fn gaps_overlaps_and_normalization() {
    let mut script = String::from("<?php\n");
    let mut labels = Vec::new();
    let mut rust = Vec::new();
    for name in [
        "UTC",
        "Europe/Brussels",
        "America/New_York",
        "Australia/Lord_Howe",
        "Pacific/Apia",
        "Pacific/Chatham",
        "Asia/Kathmandu",
        "Europe/Dublin",
    ] {
        let zone = Zone::named(name.as_bytes()).unwrap();
        script.push_str(&format!("$z=new DateTimeZone('{name}');\n"));
        for (year, month, day) in [
            (2024, 3, 10),
            (2024, 3, 31),
            (2024, 10, 27),
            (2024, 11, 3),
            (2024, 4, 7),
            (2024, 10, 6),
            (2011, 12, 30),
            (1900, 1, 1),
            (2100, 1, 1),
            (0, 1, 1),
        ] {
            for hour in 0..24 {
                for minute in [0, 15, 30, 45, 59] {
                    let civil = Civil {
                        year,
                        month,
                        day,
                        hour,
                        minute,
                        second: 0,
                    };
                    let text = format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:00");
                    script.push_str(&format!(
                        "echo (new DateTime('{text}',$z))->getTimestamp(),\"\\n\";\n"
                    ));
                    rust.push(local_timestamp(&zone, civil, None).to_string());
                    labels.push(format!("{name} {text}"));
                }
            }
        }
    }
    let php = oracle::run_php("date-resolve", &script).expect("PHP oracle required");
    oracle::compare("local wall time", &labels, &php, &rust);
}
