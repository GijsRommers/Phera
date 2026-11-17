//! Opt-in real-registry Composer differential tests.
//!
//! PHERA_PACKAGES_NETWORK=1 PHERA_BIN=... PHERA_COMPOSER=... cargo test --test network

#[test]
fn composer_fixture_comparisons() {
    if std::env::var("PHERA_PACKAGES_NETWORK").as_deref() != Ok("1") {
        return;
    }
    let binary = std::env::var_os("PHERA_BIN").expect("Set PHERA_BIN to the built Phera binary.");
    let composer =
        std::env::var_os("PHERA_COMPOSER").expect("Set PHERA_COMPOSER to Composer 2.x phar.");
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/tools/compare.py");
    let status = std::process::Command::new("python3")
        .arg(script)
        .arg(binary)
        .arg(composer)
        .status()
        .expect("Run the network comparison with Python 3 installed.");
    assert!(
        status.success(),
        "Composer comparison failed; inspect target/package-comparison logs."
    );
}
