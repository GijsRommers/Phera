//! Compare PHP's network functions without comparing host-specific addresses or flags.

use phera_extension_api::testing::scripts::Comparison;
use std::path::Path;

#[test]
fn network_calls_match_php() {
    let Some(comparison) = Comparison::from_environment("standard-network") else {
        return;
    };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scripts/network.php");
    comparison.compare_all(&[script], |_, _, _| {});
}

#[cfg(target_vendor = "apple")]
#[test]
fn ipv6_link_local_addresses_match_php() {
    let Some(comparison) = Comparison::from_environment("standard-link-local") else {
        return;
    };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scripts/network-link-local.php");
    comparison.compare_all(&[script], |_, _, _| {});
}
