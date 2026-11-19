//! Exercise public checker adoption behavior: configs, exact baselines and file exclusions.
//! Each test owns its scratch directory; no test runs the PHP files it checks.

mod check_support;
use check_support::Project;

#[test]
fn usage_errors_exit_two() {
    let project = Project::new();
    for args in [
        &[][..],
        &["--level", "2"],
        &["--format", "xml"],
        &["missing.php"],
        &["--generate-baseline"],
        &["--unknown"],
    ] {
        assert_eq!(project.run(args).status.code(), Some(2), "{args:?}");
    }
}

#[test]
fn configuration_paths_exclusions_and_vendor_are_respected() {
    let project = Project::new();
    project.write(
        "phera.toml",
        "[app]\nname = 'demo'\n[check]\nlevel = 0\npaths = [\n'src',\n]\n\
         exclude = ['src/skip/*']\n",
    );
    project.write("src/ok.php", "<?php strlen('x');");
    project.write("src/skip/bad.php", "<?php missing();");
    project.write("src/vendor/bad.php", "<?php missing();");
    assert!(project.run(&[]).status.success());
    assert!(
        project
            .run(&[project.0.join("src").to_str().unwrap()])
            .status
            .success()
    );
    assert_eq!(project.run(&["src/vendor"]).status.code(), Some(1));
}

#[test]
fn baseline_ignores_exact_entries_and_preserves_new_errors() {
    let project = Project::new();
    project.write("src.php", "<?php\nmissing();\n");
    assert_eq!(
        project
            .run(&["--generate-baseline", "baseline.json", "src.php"])
            .status
            .code(),
        Some(1)
    );
    project.write(
        "phera.toml",
        "[check]\npaths = ['src.php']\nbaseline = 'baseline.json'\n",
    );
    assert!(project.run(&[]).status.success());
    project.write("src.php", "<?php\nmissing();\nnew Missing;\n");
    let output = project.run(&["--format", "json"]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("class.notFound"));
    assert!(!text.contains("function.notFound"));
    project.write("src.php", "<?php\n\nmissing();\n");
    assert_eq!(project.run(&[]).status.code(), Some(1));
}

#[test]
fn namespace_imports_and_inherited_trait_methods_cross_files() {
    let project = Project::new();
    project.write(
        "a.php",
        "<?php namespace A; trait T { function run($a) {} }\n\
         class Base { use T; }\nconst VALUE = 1; function go($a) {}",
    );
    project.write(
        "b.php",
        "<?php namespace B; use A\\Base as ParentClass;\n\
         use function A\\go as g; use const A\\VALUE as V;\n\
         class C extends ParentClass { function f() { $this->run(V); } } g(V);",
    );
    assert!(project.run(&["."]).status.success());
}

#[test]
fn syntax_errors_do_not_hide_other_files_and_code_is_never_executed() {
    let project = Project::new();
    project.write("bad.php", "<?php function {");
    project.write(
        "good.php",
        "<?php file_put_contents('executed', 'yes'); missing();",
    );
    let output = project.run(&["--format", "json", "."]);
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(text.contains("phpstan.parse"));
    assert!(text.contains("function.notFound"));
    assert!(!project.0.join("executed").exists());
}

#[test]
fn deep_chains_are_bounded_without_crashing() {
    let project = Project::new();
    let chain = std::iter::repeat_n("isset($x)", 20_000)
        .collect::<Vec<_>>()
        .join(" && ");
    project.write("deep.php", &format!("<?php if ({chain}) {{ echo $x; }}"));
    assert!(project.run(&["deep.php"]).status.success());
}

#[test]
fn overlapping_psalm_issue_aliases_suppress_their_rules() {
    let project = Project::new();
    project.write(
        "aliases.php",
        "<?php\n/** @psalm-suppress UndefinedGlobalVariable */\necho $x;\n\
         /** @psalm-suppress UnimplementedAbstractMethod */\nclass C implements Countable {}\n\
         /** @psalm-suppress UndefinedThisPropertyFetch */\n\
         class A { function f() { echo $this->missing; } }\n",
    );
    assert!(project.run(&["aliases.php"]).status.success());
}

#[test]
fn language_level_controls_builtin_signatures() {
    let project = Project::new();
    project.write("src.php", "<?php clamp(3, 0, 5);");
    let old = project.run(&["--php", "8.5", "src.php"]);
    assert_eq!(old.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&old.stdout).contains("function.notFound"));

    project.write(
        "phera.toml",
        "[runtime]\nphp = '8.6'\n[check]\npaths = ['src.php']\n",
    );
    assert!(project.run(&[]).status.success());
    assert_eq!(project.run(&["--php", "8.5"]).status.code(), Some(1));
}

#[test]
fn language_level_is_selected_once_for_a_script_and_its_includes() {
    let project = Project::new();
    project.write(
        "src/main.php",
        "<?php require __DIR__ . '/helper.php'; echo ':' . $argv[1];",
    );
    project.write("src/helper.php", "<?php echo PHP_MINOR_VERSION;");
    let run = |flags: &[&str]| {
        project
            .command()
            .arg("run")
            .args(flags)
            .args(["src/main.php", "--php"])
            .output()
            .unwrap()
    };
    let assert_level = |flags: &[&str], expected: &str| {
        let output = run(flags);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, expected.as_bytes());
    };
    assert_level(&[], "5:--php");
    project.write("composer.json", r#"{"require":{"php":"^8.6"}}"#);
    assert_level(&[], "6:--php");
    project.write("phera.toml", "[runtime]\nphp = '8.5'\n");
    assert_level(&[], "5:--php");
    assert_level(&["--php", "8.6"], "6:--php");

    let error = run(&["--php", "8.7"]);
    assert_eq!(error.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&error.stderr).contains("Known versions are 8.5 and 8.6"));
    assert!(
        project
            .command()
            .args(["--dump", "--php", "8.6", "src/main.php"])
            .output()
            .unwrap()
            .status
            .success()
    );
}
