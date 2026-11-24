//! Exercise public checker adoption behavior: configs, exact baselines and file exclusions.
//! Each test owns its scratch directory; no test runs the PHP files it checks.

mod check_support;
use check_support::Project;

#[test]
fn usage_errors_exit_two() {
    let project = Project::new();
    project.write("src.php", "<?php");
    for args in [
        &[][..],
        &["--level", "11", "src.php"],
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
fn phpstan_baseline_suppresses_only_matching_messages_up_to_the_count() {
    let project = Project::new();
    let program = "<?php\nfunction number(int $value): int { return $value; }\n";
    project.write(
        "src.php",
        &format!("{program}echo number('one');\necho number('two');\n"),
    );
    project.write(
        "phpstan-baseline.neon",
        r#"parameters:
    ignoreErrors:
        -
            message: '#^Parameter \#1 \$value of function number expects int, string given\.$#'
            identifier: argument.type
            count: 2
            path: src.php
"#,
    );
    let args = [
        "--level",
        "max",
        "--baseline",
        "--format",
        "json",
        "src.php",
    ];
    assert!(project.run(&args).status.success());
    project.write(
        "src.php",
        &format!("{program}echo number([]);\necho number('one');\necho number('two');\necho number('three');\n"),
    );
    let output = project.run(&args);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(text.matches("argument.type").count(), 2);
    assert!(text.contains("\"line\":3"), "{text}");
    assert!(text.contains("\"line\":6"), "{text}");
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
fn source_tools_report_parser_compile_errors() {
    let project = Project::new();
    let files = [
        ("ternary.php", "<?php 1 ?: 2 ? 3 : 4;", "Unparenthesized"),
        (
            "jump.php",
            "<?php while (true) { break '2'; }",
            "operator accepts only positive integers",
        ),
        (
            "namespace.php",
            "<?php function f() {} namespace Project;",
            "Namespace declaration statement has to be the very first statement",
        ),
        (
            "mixed.php",
            "<?php namespace A; namespace B {}",
            "Cannot mix bracketed namespace declarations",
        ),
    ];
    for (file, source, _) in files {
        project.write(file, source);
    }
    for command in ["check", "lint"] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_phera"))
            .current_dir(&project.0)
            .args([command, "--format", "json", "."])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{command}");
        let text = String::from_utf8(output.stdout).unwrap();
        for (file, _, message) in files {
            assert!(text.contains(file), "{command}: {file}: {text}");
            assert!(text.contains(message), "{command}: {message}: {text}");
        }
    }
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
fn max_selects_level_ten_and_help_names_the_range() {
    let project = Project::new();
    project.write(
        "src.php",
        "<?php function useValue($value): void { $value->run(); }",
    );
    let run = |level| project.run(&["--level", level, "--format", "json", "src.php"]);
    let highest = run("10");
    let max = run("max");
    assert_eq!(highest.status.code(), Some(1));
    assert_eq!(max.status, highest.status);
    let findings = |output: std::process::Output| {
        let json = String::from_utf8(output.stdout).unwrap();
        json.split_once("\"diagnostics\":").unwrap().1.to_string()
    };
    assert_eq!(findings(max), findings(highest));
    assert_ne!(findings(run("9")), findings(run("10")));
    let help = project.run(&["--help"]);
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("0 through 10, or max")
    );
}
