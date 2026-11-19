//! Compare tables of Reflection calls with PHP 8.5, retaining individual mismatches.
//!
//! `PHERA_BIN=target/release/phera cargo run -p phera-ext-reflection --example conformance`

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use phera_extension_api::testing::oracle::run_php;
use phera_extension_api::testing::scripts::{Comparison, Runtime};

mod conformance {
    pub mod cases;
}

use conformance::cases::Surface;

const DECLARATIONS: &str = include_str!("conformance/declarations.php");
const HARNESS: &str = include_str!("conformance/harness.php");

fn main() -> Result<(), Box<dyn Error>> {
    let Some(comparison) = Comparison::from_environment("reflection") else {
        return Err("Set PHERA_BIN to the Phera executable".into());
    };
    let surfaces = conformance::cases::surfaces();
    let methods = methods(&surfaces).ok_or("PHP 8.5 oracle not available")?;
    let mut total = Counts::default();
    for surface in surfaces {
        let Some(methods) = methods.get(surface.reflector) else {
            println!("{:28} oracle class unavailable", surface.reflector);
            continue;
        };
        let counts = compare(&comparison, &surface, methods)?;
        println!(
            "{:28} {:5} calls {:5} mismatched",
            surface.reflector, counts.compared, counts.mismatches
        );
        total.compared += counts.compared;
        total.mismatches += counts.mismatches;
    }
    println!(
        "{} calls, {} mismatches. Details in {}",
        total.compared,
        total.mismatches,
        comparison.artifacts.display()
    );
    Ok(())
}

#[derive(Default)]
struct Counts {
    compared: usize,
    mismatches: usize,
}

fn methods(surfaces: &[Surface]) -> Option<BTreeMap<String, Vec<String>>> {
    let names = surfaces
        .iter()
        .map(|surface| format!("'{}'", surface.reflector))
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        "<?php foreach ([{names}] as $name) {{
            if (!class_exists($name) && !interface_exists($name)) continue;
            foreach (get_class_methods($name) as $method) echo $name, ':', $method, \"\\n\";
        }}"
    );
    let mut result: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in run_php("reflection-oracle-methods", &script)? {
        if let Some((class, method)) = line.split_once(':') {
            result.entry(class.into()).or_default().push(method.into());
        }
    }
    Some(result)
}

fn compare(
    comparison: &Comparison,
    surface: &Surface,
    methods: &[String],
) -> Result<Counts, Box<dyn Error>> {
    let path = comparison
        .artifacts
        .join(format!("{}.php", surface.reflector));
    fs::write(&path, script(surface, methods))?;
    let actual = run(comparison, &path, Runtime::Phera)?;
    let expected = run(comparison, &path, Runtime::Php)?;
    let mut mismatches = Vec::new();
    for (key, line) in &expected {
        if actual.get(key) != Some(line) {
            mismatches.push(format!(
                "{key}\n  PHP   {line}\n  Phera {}\n",
                actual.get(key).map_or("<missing>", String::as_str)
            ));
        }
    }
    let path = path.with_extension("diff");
    fs::write(path, mismatches.join(""))?;
    Ok(Counts {
        compared: expected.len(),
        mismatches: mismatches.len(),
    })
}

fn run(
    comparison: &Comparison,
    path: &Path,
    runtime: Runtime,
) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let output = comparison.command(runtime, path).output()?;
    let stdout = String::from_utf8(output.stdout)?;
    let filename = path.with_extension(format!("{}.out", runtime.label()));
    fs::write(filename, &stdout)?;
    let filename = path.with_extension(format!("{}.err", runtime.label()));
    fs::write(filename, output.stderr)?;
    let mut output = Vec::new();
    let mut results = BTreeMap::new();
    let mut started = false;
    for line in stdout.lines() {
        if line.starts_with("BEGIN ") {
            output.clear();
            started = true;
        } else if let Some((key, result)) = line
            .strip_prefix("CASE ")
            .and_then(|line| line.split_once('='))
        {
            let printed = output.join("\n");
            let result = if printed.is_empty() {
                result.to_owned()
            } else {
                format!("{result} (stdout {printed:?})")
            };
            results.insert(key.to_owned(), result);
            started = false;
        } else if started {
            output.push(line);
        }
    }
    Ok(results)
}

fn script(surface: &Surface, methods: &[String]) -> String {
    let mut source = String::from(DECLARATIONS);
    source.push_str(HARNESS);
    if let Some(setup) = surface.setup {
        source.push_str(setup);
    }
    source.push_str("\n$targets = [");
    for receiver in &surface.receivers {
        source.push_str(&format!("fn () => {receiver},\n"));
    }
    source.push_str("];\n$methods = [");
    for method in methods {
        source.push_str(&format!("'{method}' => {},\n", surface.arguments(method)));
    }
    source.push_str(
        "];\nforeach ($targets as $target => $factory) {
        foreach ($methods as $method => $calls) {
            foreach ($calls as $shape => $arguments) {
                oracleCall(\"$target:$method:$shape\", $factory, $method, $arguments);
            }
        }
    }\n",
    );
    source
}
