//! Native HashContext and namespaced Random engines must work through the real VM.
use phera_extension_api::testing::oracle;
use phera_runtime::Runtime;
#[test]
fn hashes_passwords_and_namespaced_engines() {
    let script = include_str!("hash_random.php");
    let path =
        std::env::temp_dir().join(format!("phera-hash-random-vm-{}.php", std::process::id()));
    std::fs::write(&path, script).unwrap();
    let result = Runtime::new().run_file_blocking(&path, &[], None).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        result.status,
        0,
        "{}",
        String::from_utf8_lossy(&result.output)
    );
    let php = oracle::run_php("hash-random-vm", script).expect("PHP required");
    assert_eq!(
        String::from_utf8(result.output)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        php
    );
}
