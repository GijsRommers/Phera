//! Compare binary output, warnings and exceptions from `pack()` with PHP.

use super::OracleTable;
use phera_extension_api::Value;

#[test]
fn pack_matches_php() {
    let mut table = OracleTable::new();
    let values = [
        Value::Null,
        Value::Bool(true),
        Value::Int(i64::MIN),
        Value::Int(-1),
        Value::Int(0),
        Value::Int(255),
        Value::Int(65_536),
        Value::Int(i64::MAX),
        Value::Float(-0.0),
        Value::Float(1.5),
        Value::Float(f64::INFINITY),
        Value::Float(f64::NAN),
        Value::from("12e5 suffix"),
        Value::from("7fed0"),
        Value::from(vec![b'a', 0, 255]),
        Value::from(""),
    ];
    for code in b"aAZhHcCsSnviIlLNVqQJPfgGdeExX@" {
        for repeat in ["", "0", "1", "2", "3", "7", "*"] {
            let format = Value::from(format!("{}{repeat}", *code as char));
            for value in &values {
                table.check("pack", vec![format.clone(), value.clone()]);
            }
        }
    }
    for format in [
        "",
        "C2C",
        "@3C",
        "@0C",
        "CX2C",
        "H4xC",
        "C@0C@2",
        "?",
        "x2x2147483647",
    ] {
        for length in 0..=3 {
            let mut args = vec![Value::from(format)];
            args.extend(values.iter().take(length).cloned());
            table.check("pack", args);
        }
    }
    table.check("pack", vec![]);
    table.check(
        "pack",
        vec![Value::from("C*"), Value::Int(2), Value::Int(3)],
    );
    table.finish("pack", str::to_owned);
}

#[test]
fn unpack_matches_php() {
    let mut table = OracleTable::new();
    let inputs: &[&[u8]] = &[
        b"",
        b"\xff",
        b"\0a",
        b"\xff\xff\xff\xff\xff\xff\xff\xff",
        b"ab \t\n\0\r",
        b"\0\x01\x7f\x80\xfe\xffabcdefgh",
        b"\x80\0\0\0\0\0\0\0",
    ];
    for code in b"aAZhHcCsSnviIlLNVqQJPfgGdeExX@" {
        for repeat in ["", "0", "1", "2", "3", "7", "*"] {
            for name in ["", "field", "17"] {
                let format = Value::from(format!("{}{repeat}{name}", *code as char));
                for input in inputs {
                    table.check("unpack", vec![format.clone(), Value::from(input.to_vec())]);
                }
            }
        }
    }
    for format in [
        "",
        "C/x/C",
        "Cleft/Cright",
        "C3field/Cfield1",
        "@*/Ca",
        "CX/",
        "C/@0/C",
        "C3/@1/C",
        "C/X4/C",
        "?",
        "x2147483648",
        "C//C",
    ] {
        for offset in [-1, 0, 1, 2, 3, 4] {
            table.check(
                "unpack",
                vec![Value::from(format), Value::from("abc"), Value::Int(offset)],
            );
        }
    }
    table.check("unpack", vec![]);
    table.check("unpack", vec![Value::from("C")]);
    table.finish("unpack", str::to_owned);
}
