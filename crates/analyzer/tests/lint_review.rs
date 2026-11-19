//! Default lint rules recognize a later write to `$item` after a reference foreach.

use phera_analyzer::lint::{self, Command, Options};
use std::{fs, path::Path};

fn lint_source(path: &Path, args: &[&str]) -> Vec<String> {
    let arguments = args
        .iter()
        .map(|argument| argument.to_string())
        .chain([path.display().to_string()]);
    let Command::Run(options) = Options::parse(arguments).expect("lint arguments") else {
        panic!("expected lint run");
    };
    lint::run(&options)
        .expect("lint source")
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.identifier)
        .collect()
}

#[test]
fn reference_reuse_stays_in_one_callable_and_respects_unset() {
    let directory = tempfile::tempdir().expect("scratch fixture directory");
    let path = directory.path().join("loop.php");
    let inputs = [
        (
            "function f($a) { foreach ($a as &$x) {} echo 1; $x = 2; }",
            true,
        ),
        (
            "function f($a) { foreach ($a as &$x) {} unset($x); $x = 2; }",
            false,
        ),
        (
            "function f($a) { foreach ($a as &$x) {} (function () { $x = 2; })(); }",
            false,
        ),
        (
            "function f($a, $b) { if ($b) { foreach ($a as &$x) {} } echo 1; $x = 2; }",
            true,
        ),
        (
            "function f($a, $b) { if ($b) { foreach ($a as &$x) {} } else { $x = 2; } }",
            false,
        ),
        ("function f($a) { foreach ($a as &$x) {} echo 1; }", false),
        (
            "function f($a) { foreach ($a as &$x) {} foreach ($a as &$x) {} }",
            false,
        ),
        (
            "function f($a) { switch ($a) { case 1: foreach ($a as &$x) {} break; case 2: $x = 2; } }",
            false,
        ),
    ];
    for (input, expected) in inputs {
        fs::write(&path, format!("<?php\n{input}\n")).expect("write PHP fixture");
        let findings = lint_source(&path, &[]);
        assert_eq!(
            findings
                .iter()
                .any(|rule| rule == "foreach-reference-not-unset"),
            expected,
            "{input}: {findings:?}"
        );
    }
    fs::write(&path, "<?php\nforeach ($items as &$item) {} echo 1;\n").expect("write PHP fixture");
    assert_eq!(
        lint_source(&path, &["--rule", "foreach-reference-not-unset"]),
        ["foreach-reference-not-unset"]
    );
}

#[test]
fn all_selects_opt_in_rules_and_applies_skip() {
    let directory = tempfile::tempdir().expect("scratch fixture directory");
    let path = directory.path().join("style.php");
    fs::write(&path, "<?php\nECHO 1;\n").expect("write PHP fixture");
    assert!(lint_source(&path, &[]).is_empty());
    for args in [&["--only", "all"][..], &["--all"][..]] {
        let findings = lint_source(&path, args);
        assert!(findings.iter().any(|rule| rule == "keyword-case"));
        assert!(findings.iter().any(|rule| rule == "strict-types"));
    }
    assert!(
        !lint_source(&path, &["--all", "--skip", "keyword-case"])
            .iter()
            .any(|rule| rule == "keyword-case")
    );
}

#[test]
fn strpos_fallbacks_and_unproven_values_keep_their_safe_source() {
    let directory = tempfile::tempdir().expect("scratch fixture directory");
    let path = directory.path().join("search.php");
    let source = "<?php\nnamespace Acme;\n$found = strpos($text, $needle) !== false;\n$starts = \\strpos($text, $needle) === 0;\n";
    fs::write(&path, source).expect("write PHP fixture");
    assert_eq!(
        lint_source(&path, &["--only", "modernize", "--fix"]),
        ["str-contains", "str-starts-with"]
    );
    assert_eq!(fs::read_to_string(&path).expect("read PHP fixture"), source);
    assert_eq!(
        lint_source(&path, &["--only", "modernize", "--fix-unsafe"]),
        ["str-contains"]
    );
    let changed = fs::read_to_string(&path).expect("read fixed PHP");
    assert!(changed.contains("\\str_starts_with($text, $needle)"));
    assert!(changed.contains("strpos($text, $needle) !== false"));

    let shadowed = "<?php\nnamespace Acme;\nfunction strpos($a, $b) { return false; }\n$found = strpos($text, $needle) !== false;\n";
    fs::write(&path, shadowed).expect("write shadowing function");
    assert!(lint_source(&path, &["--rule", "str-contains"]).is_empty());
}
