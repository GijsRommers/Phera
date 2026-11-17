//! Every script in `tests/scripts` runs through PHP 8.5 with phpredis and
//! through Phera, against the test Redis server, and must print the same.
//! Each script gets a key prefix of its own as its first argument.

use std::path::Path;

use phera_extension_api::testing::scripts::{Comparison, php_scripts};

#[test]
fn php_scripts_match_phera() {
    let Some(comparison) = Comparison::from_environment("redis") else {
        return;
    };
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scripts");
    let scripts = php_scripts(&directory, "");
    assert!(
        scripts.len() >= 150,
        "the Redis suite has at least 150 scripts"
    );
    let matched = comparison.compare_all(&scripts, |script, _, command| {
        let name = script.file_stem().unwrap().to_string_lossy();
        command.arg(format!("phera:{}:{name}:", std::process::id()));
    });
    let calls: usize = matched
        .iter()
        .map(|outcome| compared_calls(&outcome.stdout))
        .sum();
    eprintln!("{} scripts, {calls} compared calls", scripts.len());
}

/// `finish()` in support.php ends each script with `CALLS=<n>`, the number of
/// `check()` calls it compared.
fn compared_calls(stdout: &[u8]) -> usize {
    String::from_utf8_lossy(stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("CALLS=")?.parse::<usize>().ok())
        .sum()
}
