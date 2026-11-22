//! Compare the added standard library entry points with PHP CLI.

use std::path::Path;

use phera_extension_api::testing::scripts::Comparison;

#[test]
fn library_helpers_match_php() {
    let Some(oracle) = Comparison::from_environment("standard-library") else {
        return;
    };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle_cases/library.php");
    oracle
        .compare(&script, |_, command| {
            command.args(["-ac", "--long=answer", "tail"]);
        })
        .unwrap();
}
