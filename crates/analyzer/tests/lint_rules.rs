//! Check reported lines, messages and edited source against each rule's PHP examples.

use phera_analyzer::lint::{self, Command, Options};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn options(args: &[&str], path: &Path) -> Options {
    let args = args
        .iter()
        .map(|arg| arg.to_string())
        .chain([path.display().to_string()]);
    match Options::parse(args).expect("lint arguments") {
        Command::Run(options) => *options,
        _ => panic!("expected lint run"),
    }
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/lint")
}

#[test]
fn rule_fixtures() {
    for rule in lint::rules() {
        let root = fixtures().join(rule.name);
        let bad = root.join("bad.php");
        let good = root.join("good.php");
        let run = |file: &Path| {
            let mut settings = options(&["--rule", rule.name], file);
            if rule.group == lint::Group::Complexity {
                settings.read_config(
                    "[lint.complexity]\ncyclomatic = 2\ncognitive = 2\nnesting = 1\nfunction_lines = 2\nclass_lines = 2\nparameters = 2",
                ).expect("complexity fixture thresholds");
            }
            lint::run(&settings)
                .expect("lint diagnostics")
                .diagnostics
                .into_iter()
                .map(|diagnostic| {
                    format!(
                        "{}\t{}\t{}\n",
                        diagnostic.line, diagnostic.identifier, diagnostic.message
                    )
                })
                .collect::<String>()
        };
        let expected = fs::read_to_string(root.join("bad.expect"))
            .unwrap_or_else(|error| panic!("{}: {error}", rule.name));
        assert_eq!(run(&bad), expected, "positive fixture for {}", rule.name);
        assert_eq!(run(&good), "", "negative fixture for {}", rule.name);
    }
}

#[test]
fn fixes_reparse_and_reach_a_fixed_point() {
    let directory = std::env::temp_dir().join(format!("phera-lint-tests-{}", std::process::id()));
    fs::create_dir_all(&directory).expect("scratch fixture directory");
    let scratch = directory.join("source.php");
    for rule in lint::rules() {
        let root = fixtures().join(rule.name);
        let fixed = root.join("fixed.php");
        if !fixed.exists() {
            continue;
        }
        fs::copy(root.join("bad.php"), &scratch).expect("scratch copy of PHP fixture");
        let documented_unsafe = rule.documentation().contains("Unsafe fix.");
        if documented_unsafe {
            let safe = options(&["--rule", rule.name, "--fix"], &scratch);
            let original = fs::read(&scratch).expect("original PHP");
            let remaining = lint::run(&safe).expect("safe pass");
            assert!(!remaining.diagnostics.is_empty(), "{}", rule.name);
            assert_eq!(
                fs::read(&scratch).expect("PHP source"),
                original,
                "{}",
                rule.name
            );
        }
        let flag = if documented_unsafe {
            "--fix-unsafe"
        } else {
            "--fix"
        };
        let options = options(&["--rule", rule.name, flag], &scratch);
        let first = lint::run(&options).expect("apply rule fix");
        assert!(
            first.diagnostics.is_empty(),
            "{}: {:?}",
            rule.name,
            first.diagnostics
        );
        let expected = fs::read(&fixed).expect("expected fixed PHP");
        assert_eq!(
            fs::read(&scratch).expect("fixed PHP"),
            expected,
            "fix for {}",
            rule.name
        );
        assert!(
            phera_parser::parse_bytes(&expected).is_ok(),
            "{} fix parses",
            rule.name
        );
        let second = lint::run(&options).expect("second fix run");
        assert!(
            second.diagnostics.is_empty(),
            "{}: {:?}",
            rule.name,
            second.diagnostics
        );
        assert_eq!(
            fs::read(&scratch).expect("fixed PHP"),
            expected,
            "second fix for {}",
            rule.name
        );
    }
    fs::remove_dir_all(directory).expect("remove scratch fixtures");
}

#[test]
fn checked_in_catalog_matches_registry() {
    // The file is also the output of the public --list-rules command.
    assert_eq!(include_str!("../../../docs/lint-rules.md"), lint::catalog());
}

#[test]
fn imports_keep_comments_and_source_bytes_when_fixes_overlap() {
    let directory = tempfile::tempdir().expect("scratch lint directory");
    let scratch = directory.path().join("code.php");
    let input = b"<?php\nuse V\\{Keep, DropA, DropB}; // retained\nuse V\\Other; // phera-ignore unused-import generated reference\nnew Keep();\necho \"\xff\", __LINE__;\n";
    let expected = b"<?php\nuse V\\{Keep}; // retained\nuse V\\Other; // phera-ignore unused-import generated reference\nnew Keep();\necho \"\xff\", __LINE__;\n";
    fs::write(&scratch, input).expect("write scratch file");
    let settings = options(&["--rule", "unused-import", "--fix"], &scratch);
    let report = lint::run(&settings).expect("fix unused imports");
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(fs::read(&scratch).expect("read fixed source"), expected);
}

#[test]
fn settings_suppressions_and_baselines_share_rule_identifiers() {
    let directory = tempfile::tempdir().expect("scratch lint directory");
    let scratch = directory.path().join("code.php");
    fs::write(&scratch, "<?php\n// phera-ignore-file assignment-in-condition\nif ($value = 1) {}\n// phera-ignore self-assignment this example is intentional\n$other = $other;\n$third = $third;\n")
        .expect("write scratch file");
    let mut settings = options(&[], &scratch);
    settings
        .read_config("[lint.rules]\nself-assignment = \"warn\"")
        .expect("severity setting");
    let report = lint::run(&settings).expect("report lint");
    let [diagnostic] = report.diagnostics.as_slice() else {
        panic!("expected one finding, got {:?}", report.diagnostics);
    };
    assert_eq!(diagnostic.identifier, "self-assignment");
    assert_eq!(diagnostic.line, 6);
    assert_eq!(
        diagnostic.severity,
        phera_analyzer::diagnostic::Severity::Warn
    );
    let baseline = directory
        .path()
        .join("baseline.json")
        .to_string_lossy()
        .into_owned();
    settings.generate_baseline = Some(baseline.clone());
    lint::run(&settings).expect("generate baseline");
    settings.generate_baseline = None;
    settings.baseline = Some(baseline);
    assert!(
        lint::run(&settings)
            .expect("filter baseline")
            .diagnostics
            .is_empty()
    );
}
