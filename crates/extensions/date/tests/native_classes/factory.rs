//! DateTime factories exercise the format grammar, duration grammar and byte diagnostics.

use phera_extension_api::{PhpArray, Value, testing::oracle::php_literal};

use super::support::Matrix;

#[test]
fn format_factory_matches_php() {
    let mut matrix = Matrix::new();
    let class = "DateTimeImmutable";
    let specs = [
        ('j', "12"),
        ('d', "09"),
        ('z', "32"),
        ('S', "th"),
        ('U', "-123456789"),
        ('h', "11"),
        ('g', "9"),
        ('H', "23"),
        ('G', "7"),
        ('a', "pm"),
        ('A', "AM"),
        ('u', "123456"),
        ('v', "123"),
        ('i', "58"),
        ('m', "02"),
        ('n', "2"),
        ('s', "59"),
        ('D', "Thu"),
        ('l', "Thursday"),
        ('F', "February"),
        ('M', "Feb"),
        ('e', "European"),
        ('O', "+0545"),
        ('P', "-03:30"),
        ('p', "Z"),
        ('T', "EDT"),
        ('y', "70"),
        ('Y', "2040"),
        ('x', "+012345"),
        ('X', "-000010"),
        ('#', "-"),
        ('?', "q"),
        ('*', "discard-this"),
        (' ', " "),
        (';', ";"),
        (':', ":"),
        ('/', "/"),
        ('.', "."),
        (',', ","),
        ('-', "-"),
        ('(', "("),
        (')', ")"),
        ('!', ""),
        ('|', ""),
        ('+', "ignored"),
    ];
    for &(code, sample) in &specs {
        for prefix in ["!", "!2024 ", "!Y-m-d ", "!H:i ", "!j F Y ", "!Y z "] {
            let preamble = match prefix {
                "!2024 " => "2024 ",
                "!Y-m-d " => "2024-02-28 ",
                "!H:i " => "11:45 ",
                "!j F Y " => "29 February 2024 ",
                "!Y z " => "2024 61 ",
                _ => "",
            };
            for suffix in ["", "|", "+", r"\Z", "-e"] {
                let format = Value::from(format!("{prefix}{code}{suffix}"));
                for body in [
                    sample.as_bytes(),
                    b"",
                    b"?",
                    b"2000",
                    b"000000000000000001",
                    b"99",
                    b"Europe/Brussels",
                    b"-00:06:30",
                    b"\0",
                    b"\xff",
                    b"Tuesday",
                    b"03:04",
                    b"dqfh",
                ] {
                    let mut input = preamble.as_bytes().to_vec();
                    input.extend_from_slice(body);
                    if suffix == r"\Z" {
                        input.push(b'Z');
                    }
                    if suffix == "-e" {
                        input.extend_from_slice(b"-UTC");
                    }
                    let input = Value::from(input);
                    let result = matrix.native.call(
                        class,
                        None,
                        "createFromFormat",
                        vec![format.clone(), input.clone()],
                    );
                    let result = matrix.native.date_output(result).and_then(|value| {
                        let last = matrix.native.call(class, None, "getLastErrors", vec![])?;
                        let mut tuple = PhpArray::new();
                        tuple.push(value)?;
                        tuple.push(last)?;
                        Ok(tuple.into())
                    });
                    let expression = format!(
                        "packed({class}::createFromFormat({}, {}))",
                        php_literal(&format),
                        php_literal(&input)
                    );
                    matrix.case(expression.clone(), &expression, result);
                }
            }
        }
    }
    matrix.finish("date-format-factories");
}

#[test]
fn iso_interval_factory_matches_php() {
    let mut matrix = Matrix::new();
    let mut samples: Vec<String> = [
        "",
        " ",
        "P",
        "PT",
        "P1DT",
        "P1W2D",
        "P1D2W",
        "P1M2Y",
        "P1D1D",
        "P-1D",
        "-P1D",
        "+P1D",
        "P1.5D",
        "PT1,5S",
        "p1d",
        "P1d",
        "PT2H05M",
        "P0001-00-00T00:00:00",
        "P0001-00-00T24:59:59",
        "P0000-00-00T25:00:00",
        "R1/20240101T000000Z/20240201T000000Z",
        "2024-02-01T00:00:00Z/20240101T000000Z",
        "P2D/P2W",
        "R1/PT1S",
        "R9999999999",
        "P1D\0P2D",
        "P1D\x0bP2D",
        "\x0b",
        "P1D20240101T000000Z",
        "P1DT2SP2D",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for year in [
        "",
        "0Y",
        "1Y",
        "0001Y",
        "191Y",
        "123456789012Y",
        "1234567890123Y",
    ] {
        for month in ["", "1M", "12M", "13M", "1m", "999999999999M"] {
            for day in ["", "1D", "0D", "31D", "5W", "2W3D", "3D2W", "123456789012D"] {
                for time in [
                    "",
                    "T",
                    "T0S",
                    "T1H",
                    "T25H",
                    "T1M",
                    "T60M",
                    "T61S",
                    "T1H2M3S",
                    "T3S4H",
                    "T-1H",
                    "T999999999999S",
                    "T0000000000000S",
                    "T1H1Z",
                ] {
                    samples.push(format!("P{year}{month}{day}{time}"));
                }
            }
        }
    }
    for sample in samples {
        let value = Value::from(sample);
        let result = matrix
            .native
            .new_object("DateInterval", vec![value.clone()]);
        let result = matrix.native.interval_output(result);
        let expression = format!(
            "(new DateInterval({}))->format(INTERVAL_FORMAT)",
            php_literal(&value)
        );
        matrix.case(expression.clone(), &expression, result);
    }
    matrix.finish("date-iso-intervals");
}
