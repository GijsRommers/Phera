//! In-process medians: `cargo run --release -p phera-ext-mbstring --example benchmark`.
use phera_ext_mbstring::Mbstring;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, Registry, Value};
use std::hint::black_box;
use std::time::Instant;
fn main() {
    let mut registry = Registry::new();
    Mbstring.register(&mut registry);
    let mut context = TestContext::default();
    let pattern = "Abcé Ελληνικά 中文 😀 ";
    let repeat = 1_048_576 / pattern.len();
    let string = Value::from(pattern.repeat(repeat));
    for function in ["mb_strlen", "mb_substr", "mb_strtoupper"] {
        let mut arguments = vec![string.clone()];
        if function == "mb_substr" {
            arguments.extend([Value::Int(123), Value::Int(400_000)]);
        }
        let suffix = if function == "mb_substr" {
            ",123,400000"
        } else {
            ""
        };
        let script = format!(
            r#"<?php
$s = str_repeat('{pattern}', {repeat});
{function}($s{suffix});
for ($sample = 0; $sample < 7; $sample++) {{
    $start = hrtime(true);
    for ($i = 0; $i < 30; $i++) $out = {function}($s{suffix});
    echo (hrtime(true) - $start) / 30, "\n";
}}
"#
        );
        let lines = oracle::run_php(&format!("mbstring-benchmark-{function}"), &script).unwrap();
        let mut php: Vec<f64> = lines.iter().map(|s| s.parse().unwrap()).collect();
        php.sort_by(f64::total_cmp);
        black_box(call(&registry, &mut context, function, arguments.clone()).unwrap());
        let mut rust = Vec::new();
        for _ in 0..7 {
            let start = Instant::now();
            for _ in 0..30 {
                black_box(call(&registry, &mut context, function, arguments.clone()).unwrap());
            }
            rust.push(start.elapsed().as_nanos() as f64 / 30.0);
        }
        rust.sort_by(f64::total_cmp);
        println!(
            "{function}: {} bytes, PHP {:.3} ms, Phera {:.3} ms, ratio {:.2}",
            pattern.len() * repeat,
            php[3] / 1e6,
            rust[3] / 1e6,
            rust[3] / php[3]
        );
    }
}
