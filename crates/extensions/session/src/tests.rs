//! Malformed stored data must make `session_start()` fail without panicking.

use phera_extension_api::testing::TestContext;

use crate::codec::Format;

#[tokio::test]
async fn mutated_session_records_have_bounded_decoding() {
    const SAMPLES: usize = 110_000;
    let seeds: &[(Format, &[&[u8]])] = &[
        (
            Format::Text,
            &[
                b"a|i:1;b|R:1;",
                b"a|a:1:{i:0;r:18446744073709551615;}",
                b"x|s:9999999999999999999999:\"a\";",
                b"no delimiter",
            ],
        ),
        (
            Format::Array,
            &[
                b"a:2:{s:1:\"a\";i:1;s:1:\"b\";R:2;}",
                b"a:18446744073709551615:{i:0;N;}",
                b"S:3:\"abc\";",
                b"i:-9223372036854775809;",
            ],
        ),
        (
            Format::Binary,
            &[
                b"\x01ai:1;\x01bR:1;",
                b"\0N;",
                b"\xffshort",
                b"\x01aO:99999999999999999999:\"x\":0:{}",
            ],
        ),
    ];
    let mut rng = 0x287f_1826_ef54_1c1du64;
    for &(format, records) in seeds {
        for sample in 0..SAMPLES {
            let seed = records[sample % records.len()];
            let random = next(&mut rng);
            let mut data = seed.to_vec();
            match random % 5 {
                0 => data.truncate(next(&mut rng) as usize % data.len()),
                1 => {
                    let at = next(&mut rng) as usize % data.len();
                    data[at] = next(&mut rng) as u8;
                }
                2 => {
                    let at = next(&mut rng) as usize % data.len();
                    data.insert(at, next(&mut rng) as u8);
                }
                3 => {
                    let at = next(&mut rng) as usize % data.len();
                    data.remove(at);
                }
                _ => {
                    let length = next(&mut rng) as usize % 192;
                    data = (0..length).map(|_| next(&mut rng) as u8).collect();
                }
            }
            let mut context = TestContext::default();
            let _ = format
                .decode(&mut context, &data, "session_decode", |_, _| Ok(()))
                .await;
        }
    }
}

fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}
