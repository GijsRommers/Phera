//! PHPStan 2.3.7 reported the recorded lines, identifiers and messages.
//! Every fixture checks both the level that adds the rule and the cumulative levels.

use phera_analyzer::{check, config::Options};
use std::{collections::BTreeMap, path::Path};

type Finding = (usize, String, String);
type Oracle = BTreeMap<String, Vec<(u8, usize, String, String)>>;

#[test]
fn phpstan_level_fixtures() {
    let expected: Oracle = serde_json::from_str(include_str!("levels/phpstan-2.3.7.json")).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/levels");
    for (fixture, oracle) in expected {
        for level in 5..=10 {
            let report = check(&Options {
                level,
                paths: vec![root.join(&fixture).to_string_lossy().into_owned()],
                ..Options::default()
            })
            .unwrap();
            let actual: Vec<Finding> = report
                .diagnostics
                .into_iter()
                .map(|finding| (finding.line, finding.identifier, finding.message))
                .collect();
            let expected: Vec<Finding> = oracle
                .iter()
                .filter(|(first, ..)| *first <= level)
                .map(|(_, line, identifier, message)| (*line, identifier.clone(), message.clone()))
                .collect();
            assert_eq!(actual, expected, "{fixture}, level {level}");
        }
    }
}

#[test]
fn phpstan_core_offset_return_and_property_thresholds() {
    // PHPStan 2.3.7, with phpVersion 80500.
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/levels/offset_and_property_defaults.php");
    let oracle = [
        (
            4,
            5,
            "property.unusedType",
            "Property OptionalCount::$count (int|null) is never assigned int so it can be removed from the property type.",
        ),
        (
            2,
            23,
            "return.missing",
            "Method Lookup::find() should return int|null but return statement is missing.",
        ),
        (
            3,
            30,
            "offsetAccess.invalidOffset",
            "Invalid array key type float.",
        ),
        (
            3,
            30,
            "offsetAccess.notFound",
            "Offset float does not exist on list<string>.",
        ),
    ];
    for level in 0..=10 {
        let report = check(&Options {
            level,
            paths: vec![fixture.to_string_lossy().into_owned()],
            ..Options::default()
        })
        .unwrap();
        let actual: Vec<_> = report
            .diagnostics
            .iter()
            .map(|finding| {
                (
                    finding.line,
                    finding.identifier.as_str(),
                    finding.message.as_str(),
                )
            })
            .collect();
        let expected: Vec<_> = oracle
            .iter()
            .filter(|(first, ..)| *first <= level)
            .map(|&(_, line, identifier, message)| (line, identifier, message))
            .collect();
        assert_eq!(actual, expected, "level {level}");
    }
}

#[test]
fn phpstan_inherited_parameters_and_iterable_unions() {
    // PHPStan 2.3.7 at the level before and after each check starts.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/levels");
    for (file, threshold, line, identifier, message) in [
        (
            "phpdoc_inherited_parameters.php",
            4,
            13,
            "instanceof.alwaysFalse",
            "Instanceof between array<string>|string and Traversable will always evaluate to false.",
        ),
        (
            "foreach_union.php",
            7,
            5,
            "foreach.nonIterable",
            "Argument of an invalid type list<string>|false supplied for foreach, only iterables are supported.",
        ),
    ] {
        for level in [threshold - 1, threshold, 10] {
            let report = check(&Options {
                level,
                paths: vec![root.join(file).to_string_lossy().into_owned()],
                ..Options::default()
            })
            .unwrap();
            if level < threshold {
                assert!(report.diagnostics.is_empty(), "{file} at level {level}");
            } else {
                assert_eq!(report.diagnostics.len(), 1, "{file} at level {level}");
                let finding = &report.diagnostics[0];
                assert_eq!(
                    (
                        finding.line,
                        finding.identifier.as_str(),
                        finding.message.as_str()
                    ),
                    (line, identifier, message),
                );
            }
        }
    }
}

#[test]
fn phpstan_imports_type_aliases_across_files() {
    // PHPStan 2.3.7 follows the renamed import in Packets and the Psalm
    // import spelling in Reader. The type of the other field is a string.
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/levels/aliases");
    for level in [2, 3, 5, 10] {
        let report = check(&Options {
            level,
            paths: vec![fixture.to_string_lossy().into_owned()],
            ..Options::default()
        })
        .unwrap();
        if level == 2 {
            assert!(report.diagnostics.is_empty());
        } else {
            assert_eq!(report.diagnostics.len(), 1);
            let finding = &report.diagnostics[0];
            assert_eq!(finding.line, 13);
            assert_eq!(finding.identifier, "return.type");
            assert_eq!(
                finding.message,
                "Method AliasFixture\\Reader\\Reader::number() should return string but returns int."
            );
        }
    }
}

#[test]
fn resolved_template_in_conditional_return() {
    // PHPStan 2.3.7 with unresolvedTemplateArguments enabled reports
    // property.nonObject and return.type at line 28. With the switch
    // disabled, the generic argument resolves the return to ResultRow.
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/levels/conditional_resolved_return.php");
    let report = check(&Options {
        level: 10,
        paths: vec![fixture.to_string_lossy().into_owned()],
        ..Options::default()
    })
    .unwrap();
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
}

#[test]
fn phpstan_unused_private_method_parameter_is_opt_in() {
    // PHPStan 2.3.7 with featureToggles.unusedParameters enabled.
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/levels/unused_method_parameter.php");
    for (level, unused_parameters, reports) in
        [(3, true, false), (4, false, false), (4, true, true)]
    {
        let report = check(&Options {
            level,
            unused_parameters,
            paths: vec![fixture.to_string_lossy().into_owned()],
            ..Options::default()
        })
        .unwrap();
        if reports {
            assert_eq!(report.diagnostics.len(), 1);
            let finding = &report.diagnostics[0];
            assert_eq!(finding.line, 10);
            assert_eq!(finding.identifier, "method.unusedParameter");
            assert_eq!(
                finding.message,
                "Method Labels::normalize() has an unused parameter $limit."
            );
        } else {
            assert!(report.diagnostics.is_empty());
        }
    }
}

#[test]
fn phpstan_configured_rules() {
    // PHPStan 2.3.7 with strict-rules and bleedingEdge.neon.
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/levels/configured_rules.php");
    for level in [0, 1, 5] {
        let options = Options {
            level,
            paths: vec![path.to_string_lossy().into_owned()],
            ..Options::default()
        };
        assert!(check(&options).unwrap().diagnostics.is_empty());
        let report = check(&Options {
            strict: true,
            unnecessary_null_coalesce: true,
            ..options
        })
        .unwrap();
        let actual: Vec<_> = report
            .diagnostics
            .iter()
            .map(|finding| {
                (
                    finding.line,
                    finding.identifier.as_str(),
                    finding.message.as_str(),
                )
            })
            .collect();
        let mut expected = vec![
            (
                12,
                "foreach.keyOverwrite",
                "Foreach overwrites $key with its key variable.",
            ),
            (
                12,
                "foreach.valueOverwrite",
                "Foreach overwrites $value with its value variable.",
            ),
        ];
        if level >= 1 {
            for line in [17, 23] {
                expected.push((
                    line,
                    "nullCoalesce.unnecessary",
                    "Coalesce operator ?? is unnecessary because the left side is always set \
                     and the right side is null.",
                ));
            }
        }
        expected.push((
            39,
            "foreach.valueOverwrite",
            "Foreach overwrites $name with its value variable.",
        ));
        assert_eq!(actual, expected, "level {level}");
    }
}

#[test]
fn phpstan_late_static_contracts() {
    // PHPStan 2.3.7 at level 0.
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/levels/late_static_contracts.php");
    let report = check(&Options {
        level: 0,
        paths: vec![path.to_string_lossy().into_owned()],
        ..Options::default()
    })
    .unwrap();
    let actual: Vec<_> = report
        .diagnostics
        .iter()
        .map(|finding| {
            (
                finding.line,
                finding.identifier.as_str(),
                finding.message.as_str(),
            )
        })
        .collect();
    assert_eq!(
        actual,
        [
            (7, "new.static", "Unsafe usage of new static()."),
            (
                12,
                "classConstant.notFound",
                "Access to undefined constant static(ExtensibleFactory)::LIMIT."
            ),
        ]
    );
}

#[test]
fn phpstan_doctrine_scalar_projection() {
    // PHPStan 2.3.7 with phpstan-doctrine's AbstractQuery stub.
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/levels/doctrine_scalar_closure.php");
    let report = check(&Options {
        level: 10,
        doctrine_stubs: true,
        paths: vec![path.to_string_lossy().into_owned()],
        ..Options::default()
    })
    .unwrap();
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
}

#[test]
fn phpstan_symfony_header_defaults() {
    // PHPStan 2.3.7 with phpstan-symfony's HeaderBag return type extension.
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/levels/symfony_header_defaults.php");
    let report = check(&Options {
        level: 10,
        symfony_stubs: true,
        paths: vec![path.to_string_lossy().into_owned()],
        ..Options::default()
    })
    .unwrap();
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
}

#[test]
fn strict_loop_initializers_do_not_guarantee_later_overwrites() {
    // PHPStan 2.3.7 with strict-rules at level 0.
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/levels/strict_loops.php");
    let report = check(&Options {
        level: 0,
        strict: true,
        paths: vec![path.to_string_lossy().into_owned()],
        ..Options::default()
    })
    .unwrap();
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
}

#[test]
fn internal_methods_respect_root_namespaces_and_levels() {
    // PHPStan 2.3.7 with the bleeding-edge internalTag feature.
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/levels/internal_methods.php");
    for level in [0, 1, 2, 5] {
        let report = check(&Options {
            level,
            internal_tag: true,
            paths: vec![path.to_string_lossy().into_owned()],
            ..Options::default()
        })
        .unwrap();
        if level < 2 {
            assert!(report.diagnostics.is_empty());
        } else {
            assert_eq!(report.diagnostics.len(), 1);
            let finding = &report.diagnostics[0];
            assert_eq!(finding.line, 19);
            assert_eq!(finding.identifier, "method.internalClass");
            assert_eq!(
                finding.message,
                "Call to method run() of internal class Library\\InternalService \
                 from outside its root namespace Library."
            );
        }
    }
}
