//! CPU-time benchmark: one million `strtotime` calls, twenty formats, a fixed base.
use phera_ext_date::Date;
use phera_extension_api::testing::{TestContext, oracle};
use phera_extension_api::{Extension, NativeFn, Registry, Value};
use std::process::Command;

const INPUTS: [&str; 20] = [
    "2024-02-29",
    "2024-02-29 12:34:56",
    "2024-02-29T12:34:56+00:00",
    "2024-02-29T12:34:56.123456Z",
    "29 Feb 2024",
    "February 29, 2024",
    "02/29/2024",
    "29.02.2024",
    "20240229",
    "2024-W09-4",
    "12:34:56",
    "4:08 pm",
    "@1709206496",
    "now",
    "tomorrow",
    "+1 day",
    "-3 months",
    "next Monday",
    "last day of next month",
    "Thu, 29 Feb 2024 12:34:56 GMT",
];
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("worker") {
        let mut registry = Registry::new();
        registry.load(&phera_ext_core::Core);
        Date.register(&mut registry);
        let NativeFn::Sync(function) = registry.get_function("strtotime").unwrap().handler else {
            unreachable!()
        };
        let mut context = TestContext::default();
        let inputs: Vec<_> = INPUTS
            .iter()
            .map(|s| [Value::from(*s), Value::Int(1706704496)])
            .collect();
        let mut checksum = 0i64;
        for index in 0..1_000_000 {
            let Value::Int(value) =
                function(&mut context, std::hint::black_box(&inputs[index % 20]))
                    .map_err(|e| format!("{e:?}"))?
            else {
                return Err("invalid benchmark input".into());
            };
            checksum = checksum.wrapping_add(value);
        }
        println!("{checksum}");
        return Ok(());
    }
    let php = oracle::find_php().ok_or("PHP 8.5 oracle required")?;
    let file = std::env::temp_dir().join("phera-date-parser-benchmark.php");
    let inputs = INPUTS
        .iter()
        .map(|s| oracle::php_literal(&Value::from(*s)))
        .collect::<Vec<_>>()
        .join(",");
    std::fs::write(
        &file,
        format!(
            "<?php date_default_timezone_set('UTC'); $inputs=[{inputs}];\n\
        $sum=0; for($i=0;$i<1000000;$i++) $sum+=strtotime($inputs[$i%20],1706704496);\n\
        echo $sum,PHP_EOL;"
        ),
    )?;
    for round in 0..3 {
        for rust in if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let mut command = Command::new("/usr/bin/time");
            command.arg("-p");
            if rust {
                command.arg(std::env::current_exe()?).arg("worker");
            } else {
                command.arg(&php).arg("-n").arg(&file);
            }
            let output = command.output()?;
            println!(
                "round {round} {}: {}{}",
                if rust { "Rust" } else { "PHP" },
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            if !output.status.success() {
                return Err("benchmark failed".into());
            }
        }
    }
    Ok(())
}
