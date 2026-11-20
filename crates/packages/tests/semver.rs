//! composer/semver upstream cases, frozen from the Composer PHP oracle.
//!
//! `cargo test -p phera-packages --test semver` needs no PHP or network.

use phera_packages::version::{Constraint, Version, normalize};
use serde_json::Value;

#[test]
fn upstream_version_normalization() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/semver.json")).unwrap();
    for case in fixture["normalized"].as_array().unwrap() {
        let text = case[0].as_str().unwrap();
        assert_eq!(
            normalize(text).unwrap(),
            case[1].as_str().unwrap(),
            "{text}"
        );
    }
    for case in fixture["invalid_versions"].as_array().unwrap() {
        let text = case[0].as_str().unwrap();
        assert!(normalize(text).is_err(), "should reject {text}");
    }
}

#[test]
fn upstream_constraint_matching() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/semver.json")).unwrap();
    let mut differences = Vec::new();
    for case in fixture["constraints"].as_array().unwrap() {
        let text = case["constraint"].as_str().unwrap();
        let constraint = Constraint::parse(text).unwrap_or_else(|error| panic!("{text}: {error}"));
        for (version, expected) in case["matches"].as_object().unwrap() {
            let actual = constraint.matches(&Version::parse(version).unwrap());
            if actual != expected.as_bool().unwrap() {
                differences.push(format!(
                    "{text} / {version}: got {actual}, expected {expected}"
                ));
            }
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

#[test]
fn upstream_invalid_constraints() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/semver.json")).unwrap();
    let mut accepted = Vec::new();
    for case in fixture["invalid_constraints"].as_array().unwrap() {
        let text = case[0].as_str().unwrap();
        if Constraint::parse(text).is_ok() {
            accepted.push(text);
        }
    }
    assert!(accepted.is_empty(), "incorrectly accepted: {accepted:?}");
}

#[test]
fn upstream_satisfies_cases() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/semver.json")).unwrap();
    for case in fixture["satisfies"].as_array().unwrap() {
        let expected = case[0].as_bool().unwrap();
        let version = case[1].as_str().unwrap();
        let text = case[2].as_str().unwrap();
        assert_eq!(
            Constraint::parse(text)
                .unwrap()
                .matches(&Version::parse(version).unwrap()),
            expected,
            "{version} / {text}"
        );
    }
}
