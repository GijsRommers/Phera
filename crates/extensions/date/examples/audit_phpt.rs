//! Compare selected upstream date FILE sections against PHP: `date('Y', 0)` → `1970`.
//! Usage: cargo run -p phera-ext-date --example audit_phpt -- PHP_SRC PHERA_BIN
//! This audit retains all failures, including unsupported engine features and INI sections.
use phera_extension_api::testing::oracle::find_php;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.len() != 2 {
        return Err("expected PHP_SRC and PHERA_BIN".into());
    }
    let php = find_php().ok_or("PHP 8.5 oracle required")?;
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target/date-phpt-audit");
    std::fs::create_dir_all(&directory)?;
    let mut report = String::new();
    let (mut passed, mut failed, mut skipped) = (0, 0, 0);
    for name in include_str!("../tests/upstream-selection.txt").lines() {
        let source = Path::new(&arguments[0]).join("ext/date/tests").join(name);
        let phpt = std::fs::read_to_string(&source)?;
        let body = section(&phpt, "FILE").ok_or("missing FILE")?;
        if section(&phpt, "INI").is_some() || body.contains("getenv(") {
            skipped += 1;
            report.push_str(&format!(
                "SKIP {name}: INI or environment setup unsupported by audit\n"
            ));
            continue;
        }
        let file = directory.join(name.replace(".phpt", ".php"));
        std::fs::write(&file, body)?;
        let oracle = Command::new(&php)
            .args(["-n", "-d", "error_reporting=-1"])
            .arg(&file)
            .output()?;
        let runtime = Command::new(&arguments[1]).arg(&file).output()?;
        let matches = oracle.status.code() == runtime.status.code()
            && oracle.stdout == runtime.stdout
            && oracle.stderr == runtime.stderr;
        if matches {
            passed += 1;
        } else {
            failed += 1;
        }
        report.push_str(&format!(
            "{} {name}\n",
            if matches { "PASS" } else { "FAIL" }
        ));
        std::fs::write(file.with_extension("php.out"), oracle.stdout)?;
        std::fs::write(file.with_extension("phera.out"), runtime.stdout)?;
        std::fs::write(file.with_extension("php.err"), oracle.stderr)?;
        std::fs::write(file.with_extension("phera.err"), runtime.stderr)?;
    }
    report.push_str(&format!(
        "{passed} matched, {failed} mismatched, {skipped} unsupported setup\n"
    ));
    print!("{report}");
    std::fs::write(directory.join("report.txt"), report)?;
    if failed != 0 {
        return Err(
            "upstream FILE comparisons failed; evidence retained in target/date-phpt-audit".into(),
        );
    }
    Ok(())
}

fn section<'a>(contents: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("--{name}--\n");
    let (_, rest) = contents.split_once(&marker)?;
    Some(rest.split_once("\n--").map_or(rest, |(body, _)| body))
}
