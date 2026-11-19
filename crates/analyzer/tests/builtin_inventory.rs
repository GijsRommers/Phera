//! Verify the generated table against the selected PHP oracle's default build.
//! Runtime functions, classes and constants must all be known to static analysis.

use phera_analyzer::{builtins, index::Signature};
use phera_extension_api::testing::oracle::{find_php, test_level};
use std::process::Command;

fn native_inventory() -> String {
    let php = find_php().expect("The selected PHP oracle is required");
    let output = Command::new(php)
        .args([
            "-n",
            "-r",
            r#"
        if (PHP_MAJOR_VERSION . "." . PHP_MINOR_VERSION !== $argv[1]) { exit(2); }
        foreach (get_defined_functions()['internal'] as $name) {
            $f = new ReflectionFunction($name);
            $parameters = array_map(fn($p) => $p->name, $f->getParameters());
            echo "F\t$name\t", implode(',', $parameters), "\n";
        }
        foreach (array_merge(get_declared_classes(), get_declared_interfaces(),
                 get_declared_traits()) as $name) {
            // phpredis is a PECL extension, not part of php-src's stubs; the
            // oracle loads it for the Redis comparisons.
            if ((new ReflectionClass($name))->getExtensionName() === 'redis') { continue; }
            echo "C\t$name\n";
            foreach ((new ReflectionClass($name))->getMethods() as $method) {
                echo "M\t$name::", $method->name, "\t",
                     implode(',', array_map(fn($p) => $p->name,
                         $method->getParameters())), "\n";
            }
        }
        foreach (array_keys(get_defined_constants()) as $name) { echo "K\t$name\n"; }
    "#,
            test_level().name(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "The selected PHP oracle failed: {:?}",
        output.status
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn default_php_symbols_and_parameter_names_are_present() {
    let index = builtins::load_at(test_level());
    let mut missing = Vec::new();
    for line in native_inventory().lines() {
        let fields: Vec<_> = line.split('\t').collect();
        match fields[0] {
            "F" => check_parameters(
                index.functions.get(&fields[1].to_ascii_lowercase()),
                &fields,
                &mut missing,
            ),
            "M" => {
                let (class, method) = fields[1].split_once("::").unwrap();
                check_parameters(index.method(class, method), &fields, &mut missing);
            }
            "C" if index.class(fields[1]).is_none() => missing.push(line.to_string()),
            "K" if !index.constants.contains_key(fields[1]) => missing.push(line.to_string()),
            _ => {}
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

fn check_parameters(signature: Option<&Signature>, fields: &[&str], missing: &mut Vec<String>) {
    let Some(signature) = signature else {
        missing.push(format!("Missing {}", fields[1]));
        return;
    };
    let parameters = signature
        .parameters
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    if parameters != fields[2] {
        missing.push(format!(
            "{} parameter names: {parameters} != {}",
            fields[1], fields[2]
        ));
    }
}
