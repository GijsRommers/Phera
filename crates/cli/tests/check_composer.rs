//! Composer metadata supplies declarations without running its PHP or reporting its bodies.

mod check_support;
use check_support::Project;

const PREAMBLE: &str =
    "<?php $vendorDir = dirname(dirname(__FILE__)); $baseDir = dirname($vendorDir);";

fn map(project: &Project, name: &str, entries: &str) {
    project.write(
        &format!("vendor/composer/autoload_{name}.php"),
        &format!("{PREAMBLE} return [{entries}];"),
    );
}

fn composer_project() -> Project {
    let project = Project::new();
    project.write("composer.json", "{}");
    map(&project, "classmap", "");
    map(&project, "psr4", "");
    project
}

#[test]
fn composer_loads_helpers_and_class_ancestry_on_demand() {
    let project = composer_project();
    map(
        &project,
        "classmap",
        r"'Library\\Base' => $vendorDir . '/package/unusual.php',
           'Library\\Unused' => $vendorDir . '/package/Unused.php'",
    );
    map(
        &project,
        "psr4",
        r"'Library\\' => [$vendorDir . '/empty', $vendorDir . '/package']",
    );
    map(
        &project,
        "files",
        r"'hash' => $vendorDir . '/package/helpers.php'",
    );
    project.write(
        "vendor/package/helpers.php",
        "<?php if (!function_exists('helper')) { function helper() {} } const DEFAULT_VALUE = 1;",
    );
    project.write(
        "vendor/package/Client.php",
        "<?php namespace Library; class Client extends Base {}",
    );
    project.write(
        "vendor/package/unusual.php",
        "<?php namespace Library; class Base { function read($value) {} } missing();",
    );
    project.write("vendor/package/Unused.php", "<?php never_loaded();");
    project.write(
        "src/check.php",
        r"<?php use Library\Client; (new Client)->read(DEFAULT_VALUE); helper();",
    );
    let output = project.run(&["--format", "json", "src"]);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{text}");
    assert!(text.contains(r#""vendor_files":3"#), "{text}");
    project.write("src/check.php", r"<?php (new Library\Client)->read();");
    let output = project.run(&["src"]);
    let text = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("[arguments.count]"), "{text}");
    assert!(
        text.contains("1 findings in 1 of 1 files, level 1,"),
        "{text}"
    );
    assert_eq!(
        project.run(&["vendor/package/unusual.php"]).status.code(),
        Some(1)
    );
}

#[test]
fn scan_directories_supply_declarations_and_exclusions_still_apply() {
    let project = Project::new();
    project.write(
        "phera.toml",
        "[check]\npaths = ['src']\nscan = ['legacy/lib']\nexclude = ['legacy/lib/ignored.php']\n",
    );
    project.write("src/check.php", "<?php supported(); ignored();");
    project.write(
        "legacy/lib/known.php",
        "<?php function supported() {} missing();",
    );
    project.write("legacy/lib/ignored.php", "<?php function ignored() {}");
    let output = project.run(&[]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Function ignored"), "{text}");
    assert!(text.contains("1 findings in 1 of 1 files"), "{text}");
}

#[test]
fn malformed_metadata_and_psr_zero_name_the_map() {
    let project = composer_project();
    project.write("check.php", "<?php strlen('test');");
    map(
        &project,
        "classmap",
        "'C' => file_put_contents('executed', 'yes')",
    );
    let output = project.run(&["check.php"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("autoload_classmap.php"));
    assert!(!project.0.join("executed").exists());
    map(&project, "classmap", "");
    map(
        &project,
        "namespaces",
        "'Legacy' => [$vendorDir . '/legacy']",
    );
    let output = project.run(&["check.php"]);
    let error = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2));
    assert!(error.contains("autoload_namespaces.php"), "{error}");
    assert!(
        error.contains("PSR-0 autoloading is not supported"),
        "{error}"
    );
}
