//! Composer differential tests for the 25 original projects.
//!
//! Run with `PHERA_PACKAGES_NETWORK=1 PHERA_BIN=… PHERA_COMPOSER=… cargo test --test network`.

mod composer;

#[test]
fn composer_fixture_comparisons() {
    if std::env::var("PHERA_PACKAGES_NETWORK").as_deref() != Ok("1") {
        eprintln!("Set PHERA_PACKAGES_NETWORK=1 to run the Composer registry comparison.");
        return;
    }
    composer::compare().unwrap();
}
