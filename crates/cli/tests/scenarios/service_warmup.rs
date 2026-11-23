//! `phera serve --no-service-warmup` reaches the framework adapters as an
//! environment variable, the way `APP_RUNTIME` reaches symfony/runtime:
//!
//! ```php
//! // in Phera's Laravel warm-up and Symfony runner
//! if (($_SERVER['PHERA_SERVICE_WARMUP'] ?? '') === '0') { return; }
//! ```

use super::super::Served;

const INDEX: &str = "<?php echo $_SERVER['PHERA_SERVICE_WARMUP'] ?? 'not set';";

#[test]
fn the_option_sets_the_variable_the_adapters_read() {
    let on = Served::start("service-warmup-on", &[("index.php", INDEX)]);
    assert_eq!(on.get("/"), (200, "not set".to_string()));

    let off = Served::start_in(
        "service-warmup-off",
        1,
        &[("index.php", INDEX)],
        ".",
        &["--no-service-warmup"],
    );
    assert_eq!(off.get("/"), (200, "0".to_string()));
}
