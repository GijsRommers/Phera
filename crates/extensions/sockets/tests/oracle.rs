//! Compare loopback I/O without relying on port numbers or operating system flags.

use std::path::Path;

use phera_extension_api::testing::scripts::Comparison;

#[test]
fn sockets_match_php_on_loopback() {
    let Some(oracle) = Comparison::from_environment("sockets") else {
        return;
    };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle_cases/loopback.php");
    oracle.compare_all(&[script], |_, _, _| {});
}
