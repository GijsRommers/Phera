//! A PHP 8.6 change may still need its own oracle expectation.
//!
//! For example, `spl_object_hash($object)` emits a deprecation at PHP 8.6.

use phera_extension_api::LanguageLevel;

use super::Outcome;

pub(super) fn expected_outcome(outcome: Outcome, name: &str, level: LanguageLevel) -> Outcome {
    if level != LanguageLevel::Php86 || std::env::var_os("PHERA_PHPT_BIN").is_some() {
        return outcome;
    }
    if matches!(outcome, Outcome::Pass)
        && let Some(reason) = changes().get(name)
    {
        // The shared expectation still describes PHP 8.5. A pass against it
        // can also disagree with the measured PHP 8.6 behavior.
        return Outcome::ExpectedFail((*reason).into());
    }
    outcome
}

fn changes() -> &'static std::collections::HashMap<&'static str, &'static str> {
    static CHANGES: std::sync::OnceLock<std::collections::HashMap<&str, &str>> =
        std::sync::OnceLock::new();
    CHANGES.get_or_init(|| {
        include_str!("../../../../tests/phpt/xfail-php86.tsv")
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.split_once('\t'))
            .collect()
    })
}
