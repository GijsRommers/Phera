//! Run the same stateful PHP script through the CLI and the PHP oracle.

mod script_cases;

use std::path::Path;

use phera_extension_api::testing::scripts::{Comparison, php_scripts};

#[test]
fn scripts_match_php() {
    let Some(comparison) = Comparison::from_environment("streams") else {
        return;
    };
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scripts");
    let mut scripts = php_scripts(&directory, "");
    let temporary = tempfile::tempdir().expect("create isolated script directories");
    let generated = temporary.path().join("generated");
    std::fs::create_dir(&generated).expect("create generated script directory");
    scripts.extend(script_cases::generate(&generated));
    assert!(scripts.len() >= 80);
    comparison.compare_all(&scripts, |script, runtime, command| {
        let name = script.file_stem().expect("script name").to_string_lossy();
        let directory = temporary.path().join(format!("{name}-{}", runtime.label()));
        std::fs::create_dir(&directory).expect("create script working directory");
        command.current_dir(directory);
    });
}
