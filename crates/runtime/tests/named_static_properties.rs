//! Static properties of a class the source names (`Post::$count`) find
//! their class by its name key; what a program sees must be what PHP shows.

use phera_runtime::Runtime;

const SCRIPT: &str = r#"<?php
$file = sys_get_temp_dir() . '/phera-named-statics-' . getmypid() . '.php';
file_put_contents($file, '<?php class Lazy { public static $hits = 0; public static int $typed = 1; }');
spl_autoload_register(function ($class) use ($file) {
    echo "autoload $class\n";
    if ($class === 'Lazy') { require $file; }
});
echo Lazy::$hits, "\n";
Lazy::$hits++; Lazy::$hits += 2; echo Lazy::$hits, "\n";
$ref = &Lazy::$hits; $ref = 10; echo Lazy::$hits, "\n";
Lazy::$typed = "42"; var_dump(Lazy::$typed);
try { Lazy::$typed = "no"; } catch (TypeError $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
function later() { return Later::$value; }
try { echo later(), "\n"; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
if (true) { class Later { public static $value = 'later'; } }
echo later(), "\n";
class Base { public static $count = 0; protected static $hidden = 'h'; private static $secret = 's';
    static function bump() { return ++static::$count; } static function hidden() { return static::$hidden . self::$secret; } }
class Child extends Base {}
Child::$count++; echo Base::$count, ' ', Child::bump(), ' ', Base::$count, ' ', Child::hidden(), "\n";
trait Counter { public static $traitCount = 1; }
class UsesCounter { use Counter; }
echo UsesCounter::$traitCount, "\n";
foreach (['Missing::$x' => fn () => Missing::$x, 'Base::$nope' => fn () => Base::$nope, 'Base::$secret' => fn () => Base::$secret] as $what => $read) {
    try { $read(); } catch (Error $e) { echo $what, ' -> ', get_class($e), ': ', $e->getMessage(), "\n"; }
}
unlink($file);
"#;

#[test]
fn named_static_properties_behave_as_in_php() {
    let Some(expected) =
        phera_extension_api::testing::oracle::run_php("named-static-properties", SCRIPT)
    else {
        eprintln!("SKIP: PHP 8.5 is not available");
        return;
    };
    let dir = std::env::temp_dir().join(format!("phera-named-statics-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("test directory");
    let script = dir.join("index.php");
    std::fs::write(&script, SCRIPT).expect("script");
    let result = Runtime::new()
        .run_file_blocking(&script, &[], None)
        .expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    let output = String::from_utf8_lossy(&result.output);
    let got: Vec<&str> = output.lines().collect();
    assert!(expected.len() > 10, "PHP printed {expected:?}");
    assert_eq!(got, expected.iter().map(String::as_str).collect::<Vec<_>>());
}
