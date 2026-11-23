//! Constants of a class the source names (`Post::LIMIT`) are found by the
//! class's name key; what a program sees must be what PHP shows: the
//! autoloader on first use, a class declared later, `static::` in an
//! inherited method, constants evaluated on first read, and the errors.

use phera_runtime::Runtime;

const SCRIPT: &str = r#"<?php
$file = sys_get_temp_dir() . '/phera-named-constants-' . getmypid() . '.php';
file_put_contents($file, '<?php class Lazy { const V = "loaded"; const W = self::V . "+w"; }');
spl_autoload_register(function ($class) use ($file) {
    echo "autoload $class\n";
    if ($class === 'Lazy') { require $file; }
});
echo Lazy::V, "\n", Lazy::W, "\n", lazy::V, "\n";
function later() { return Later::X; }
try { echo later(), "\n"; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
if (true) { class Later { const X = 'later'; } }
echo later(), "\n";
class Base { const N = 'base'; const DOUBLE = PHP_INT_SIZE * 2; function name() { return static::N; } }
class Child extends Base { const N = 'child'; }
echo (new Child)->name(), ' ', (new Base)->name(), ' ', Child::DOUBLE, "\n";
foreach (['Missing::X' => fn () => Missing::X, 'Base::NOPE' => fn () => Base::NOPE, 'Base::$nope' => fn () => Base::$nope] as $what => $read) {
    try { $read(); } catch (Error $e) { echo $what, ' -> ', get_class($e), ': ', $e->getMessage(), "\n"; }
}
unlink($file);
"#;

#[test]
fn named_class_constants_behave_as_in_php() {
    let Some(expected) =
        phera_extension_api::testing::oracle::run_php("named-class-constants", SCRIPT)
    else {
        eprintln!("SKIP: PHP 8.5 is not available");
        return;
    };
    let dir =
        std::env::temp_dir().join(format!("phera-named-constants-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("test directory");
    let script = dir.join("index.php");
    std::fs::write(&script, SCRIPT).expect("script");
    let result = Runtime::new()
        .run_file_blocking(&script, &[], None)
        .expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    let output = String::from_utf8_lossy(&result.output);
    let got: Vec<&str> = output.lines().collect();
    assert_eq!(got, expected.iter().map(String::as_str).collect::<Vec<_>>());
}
