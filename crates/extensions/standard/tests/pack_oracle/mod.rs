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
