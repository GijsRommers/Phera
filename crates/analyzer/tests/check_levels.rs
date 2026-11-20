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
