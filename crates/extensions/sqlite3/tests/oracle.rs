//! Every script in `tests/scripts`, for the SQLite3 classes, runs through
//! PHP 8.5 and through Phera, each in a directory of its own for its
//! database files, and must print the same bytes.

use std::path::Path;

use phera_extension_api::testing::scripts::{Comparison, php_scripts};

#[test]
fn sqlite_scripts_match_php() {
    let Some(comparison) = Comparison::from_environment("sqlite3") else {
        return;
    };
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scripts");
    comparison.compare_all(&php_scripts(&directory, ""), |script, runtime, command| {
        let data = comparison.scratch_directory(script, runtime);
        command.current_dir(&data).arg(&data);
    });
}
