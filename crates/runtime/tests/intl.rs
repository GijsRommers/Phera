//! IntlChar name enumeration through PHP callbacks and the real runtime.

use phera_extension_api::testing::oracle;
use phera_runtime::Runtime;

#[test]
fn intl_character_name_callbacks() {
    let script = r#"<?php
foreach ([[65, 68, 0], [0, 3, 2], [0x1a2, 0x1a3, 3], [65, 68, 4], [4, 0, 2]]
         as [$start, $end, $type]) {
    $names = [];
    $result = IntlChar::enumCharNames($start, $end, static function (...$args) use (&$names) {
        $names[] = $args;
        return false;
    }, $type);
    echo json_encode([$result, $names, intl_get_error_message()]), "\n";
}
try {
    IntlChar::enumCharNames("A", "D", static function ($cp) {
        throw new RuntimeException("callback at $cp");
    });
} catch (RuntimeException $e) {
    echo $e->getMessage(), "\n";
}
"#;
    let php = oracle::run_php("intl-character-name-callbacks", script).expect("PHP 8.5 required");
    let path = std::env::temp_dir().join(format!("phera-intl-runtime-{}.php", std::process::id()));
    std::fs::write(&path, script).unwrap();
    let result = Runtime::new().run_file_blocking(&path, &[], None);
    std::fs::remove_file(path).unwrap();
    let result = result.unwrap();
    let output = String::from_utf8_lossy(&result.output);
    assert_eq!(result.status, 0, "{output}");
    assert_eq!(output.lines().collect::<Vec<_>>(), php);
}
