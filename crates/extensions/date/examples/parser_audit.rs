//! Audit upstream parser PHPTs with a frozen procedural clock, preserving all failures.
//! Usage: cargo run -p phera-ext-date --example parser_audit -- PHP_SRC PHERA_BIN
use phera_extension_api::testing::oracle::find_php;
use phera_phpt::{Phpt, expectf_matches, find_phpt_files};
use std::{fs, path::PathBuf, process::Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("expected PHP_SRC PHERA_BIN".into());
    }
    let php = find_php().ok_or("PHP 8.5 oracle required")?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = root.join("../../../target/date-parser-phpt");
    fs::create_dir_all(&output)?;
    let prepare = root.join("examples/parser_audit/prepare.php");
    let mut counts = [0; 5];
    let mut report = String::new();
    for path in find_phpt_files(&PathBuf::from(&args[0]).join("ext/date/tests")) {
        let phpt = Phpt::parse_file(&path)?;
        let Some(body) = phpt.section("FILE") else {
            continue;
        };
        if !body.contains("strtotime(") && !body.contains("date_parse(") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy();
        let class = [
            "new Date",
            "date_create",
            "date_format(",
            "date_timestamp_get(",
            "date_modify(",
            "date_timezone_",
            "DateTime::",
            "DateTimeImmutable::",
        ]
        .iter()
        .any(|s| body.contains(s));
        let other = [
            "date_sun_",
            "date_sunrise",
            "date_sunset",
            "IntlDateFormatter",
            "ini_set(",
            "ini_get(",
            "getenv(",
            "putenv(",
        ]
        .iter()
        .find(|s| body.contains(**s));
        if class || other.is_some() {
            counts[if class { 2 } else { 3 }] += 1;
            report.push_str(&format!(
                "{} {name}: {}\n",
                if class { "CLASSES" } else { "UNSUPPORTED" },
                other.copied().unwrap_or("DateTime API")
            ));
            continue;
        }
        if let Some(skip) = phpt.section("SKIPIF") {
            let skip_file = output.join(format!("{name}.skip.php"));
            fs::write(&skip_file, skip)?;
            let skipped = Command::new(&php).arg("-n").arg(&skip_file).output()?;
            let message = String::from_utf8_lossy(&skipped.stdout);
            if message
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("skip")
            {
                counts[4] += 1;
                report.push_str(&format!("SKIP {name}: {}\n", message.trim()));
                continue;
            }
        }
        let zone = phpt
            .section("INI")
            .unwrap_or("")
            .lines()
            .find_map(|l| l.trim().strip_prefix("date.timezone="))
            .unwrap_or("UTC")
            .trim_matches('"');
        let raw = output.join(format!("{name}.raw.php"));
        fs::write(&raw, body)?;
        let prepared = Command::new(&php)
            .arg("-n")
            .arg(&prepare)
            .arg(&raw)
            .arg(zone)
            .output()?;
        if !prepared.status.success() {
            return Err(String::from_utf8_lossy(&prepared.stderr).into());
        }
        let file = output.join(format!("{name}.php"));
        fs::write(&file, prepared.stdout)?;
        let reference = Command::new(&php)
            .args([
                "-n",
                "-d",
                "error_reporting=-1",
                "-d",
                "fatal_error_backtraces=0",
            ])
            .arg(&file)
            .output()?;
        let actual = Command::new(&args[1]).arg(&file).output()?;
        let matches = reference.stdout == actual.stdout
            && reference.stderr == actual.stderr
            && reference.status.code() == actual.status.code();
        let php_text = String::from_utf8_lossy(&reference.stdout);
        let expected = if let Some(expected) = phpt.section("EXPECT") {
            expected.trim() == php_text.trim()
        } else if let Some(expected) = phpt.section("EXPECTF") {
            expectf_matches(expected.trim(), php_text.trim()).unwrap_or(false)
        } else {
            false
        };
        counts[usize::from(!matches)] += 1;
        report.push_str(&format!(
            "{} {name}: {}\n",
            if matches { "PASS" } else { "FAIL" },
            if expected {
                "upstream expectation matches PHP"
            } else {
                "PHP differs from upstream expectation with frozen clock/current tzdb"
            }
        ));
        for (suffix, bytes) in [
            ("php.out", reference.stdout),
            ("rust.out", actual.stdout),
            ("php.err", reference.stderr),
            ("rust.err", actual.stderr),
        ] {
            fs::write(output.join(format!("{name}.{suffix}")), bytes)?;
        }
    }
    report.push_str(&format!(
        "{} matched, {} failed, {} need classes, {} other dependencies, {} platform skips\n",
        counts[0], counts[1], counts[2], counts[3], counts[4]
    ));
    print!("{report}");
    fs::write(output.join("report.txt"), report)?;
    if counts[1] > 0 {
        return Err("upstream failures retained in target/date-parser-phpt".into());
    }
    Ok(())
}
