//! The hash and random extensions through the real VM, compared with PHP:
//! `HashContext` objects, the namespaced engine classes, password hashing
//! off the worker thread, and the request's shared Mt19937.

use std::path::Path;

use phera_extension_api::testing::oracle;
use phera_runtime::Runtime;

#[test]
fn hashes_passwords_and_namespaced_engines() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/hash_random.php");
    let result = Runtime::new()
        .run_file_blocking(&script, &[], None)
        .unwrap();
    let output = String::from_utf8_lossy(&result.output);
    assert_eq!(result.status, 0, "{output}");
    let php = oracle::run_php("hash-random-vm", include_str!("hash_random.php"))
        .expect("PHP 8.5 required");
    assert_eq!(output.lines().collect::<Vec<_>>(), php);
}
