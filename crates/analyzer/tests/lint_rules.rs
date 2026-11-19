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
            let mut args = vec!["--rule", rule.name];
            if rule.name.starts_with("php86-") {
                args.extend(["--php", "8.6"]);
            }
            lint::run(&options(&args, file))
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
        let options = options(&["--rule", rule.name, "--fix-unsafe"], &scratch);
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
