//! End-to-end parity tests for Breakthrough 9:
//! - In-loop `SiteOutcome::MagicCall` (`__call`)
//! - Synchronous frame-stack `__get` / `__set` with recursion guard & unwind cleanup
//! - Namespaced `GuardBuiltin` -> `CallNativeDirect` / `PureNative` + late shadow
//! - Optimizer unlocked on `try` / `catch` / `finally` functions
//! - Stay-in-loop `NativeLoop` (`array_map`, `array_filter`, `array_reduce`) + sync 1-arg `array_filter`

use phera_runtime::Runtime;

fn run_script(name: &str, script: &str) -> String {
    let dir = std::env::temp_dir().join(format!("phera-bt9-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("test directory");
    let path = dir.join("index.php");
    std::fs::write(&path, script).expect("script");
    let result = Runtime::new()
        .run_file_blocking(&path, &[], None)
        .expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    String::from_utf8_lossy(&result.output).into_owned()
}

#[test]
fn magic_call_and_sync_magic_properties_preserve_guards_and_unwinding() {
    const SCRIPT: &str = r#"<?php
class Model {
    private array $attrs = [];
    public int $calls = 0;

    public function __get(string $key): mixed {
        if ($key === 'boom') {
            throw new RuntimeException('get boom');
        }
        if ($key === 'recurse') {
            // Re-entering __get on the same property must respect PHP's recursion guard.
            return @$this->recurse ?? 'guarded';
        }
        return $this->attrs[$key] ?? null;
    }

    public function __set(string $key, mixed $value): void {
        if ($key === 'bad') {
            throw new RuntimeException('set boom');
        }
        $this->attrs[$key] = $value;
    }

    public function __call(string $name, array $args): string {
        $this->calls++;
        if ($name === 'fail') {
            throw new RuntimeException('call boom');
        }
        return $name . ':' . implode(',', $args);
    }
}

$m = new Model();
for ($i = 0; $i < 5; $i++) {
    $m->title = 'post-' . $i;
    $last = $m->title;
}
echo $last, "\n";
echo $m->recurse, "\n";
try {
    $x = $m->boom;
} catch (RuntimeException $e) {
    echo $e->getMessage(), "\n";
}
// Guard must have been popped on unwind so reading 'boom' again still hits __get.
try {
    $x = $m->boom;
} catch (RuntimeException $e) {
    echo $e->getMessage(), "2\n";
}
try {
    $m->bad = 1;
} catch (RuntimeException $e) {
    echo $e->getMessage(), "\n";
}
echo $m->where('id', 42), "\n";
echo $m->where('status', 'published'), "\n";
try {
    $m->fail('x');
} catch (RuntimeException $e) {
    echo $e->getMessage(), "\n";
}
echo "calls=", $m->calls, "\n";
"#;

    let out = run_script("magic", SCRIPT);
    assert_eq!(
        out,
        "post-4\nguarded\nget boom\nget boom2\nset boom\nwhere:id,42\nwhere:status,published\ncall boom\ncalls=3\n"
    );
}

#[test]
fn namespaced_direct_natives_and_pure_natives_respect_late_shadows() {
    const SCRIPT: &str = r#"<?php
namespace App;

function probe(string $s, array $a, array $b): string {
    $p = strpos($s, 'world');
    $ok = in_array('b', ['a', 'b', 'c'], true) ? '1' : '0';
    $has = array_key_exists('k', $a) ? '1' : '0';
    $merged = array_merge($a, $b);
    $keys = implode(',', array_keys($merged));
    $vals = implode(',', array_values($merged));
    $upper = strtoupper('abC1');
    $filtered = implode(',', array_filter([0, 'x', '', null, 'y']));
    return "$p|$ok|$has|$keys|$vals|$upper|$filtered";
}

function fresh(string $s): mixed {
    return strpos($s, 'world');
}

echo probe('hello world', ['k' => 1, 5 => 'a'], ['k' => 2, 9 => 'b']), "\n";

// Declare a namespace-local shadow conditionally AFTER probe() has already bound to global strpos.
if (true) {
    function strpos(string $h, string $n): string {
        return "shadow:$h:$n";
    }
}

echo probe('hello world', ['k' => 1], ['m' => 3]), "\n";
echo fresh('hello world'), "\n";
"#;

    let out = run_script("ns-natives", SCRIPT);
    assert_eq!(
        out,
        "6|1|1|k,0,1|2,a,b|ABC1|x,y\n6|1|1|k,m|1,3|ABC1|x,y\nshadow:hello world:world\n"
    );
}

#[test]
fn native_loops_stay_in_loop_and_handle_coercion_and_exceptions() {
    const SCRIPT: &str = r#"<?php
$nums = [1, 2, 3, 4, 5];
$doubled = array_map(fn (int $n): int => $n * 2, $nums);
echo implode(',', $doubled), "\n";

$pairs = array_map(fn (int $a, int $b): int => $a + $b, [1, 2, 3], [10, 20, 30]);
echo implode(',', $pairs), "\n";

$evens = array_filter($nums, fn (int $n): bool => $n % 2 === 0);
echo implode(',', array_keys($evens)), ':', implode(',', $evens), "\n";

$byKey = array_filter(['a' => 1, 'b' => 2, 'c' => 3], fn (string $k): bool => $k !== 'b', ARRAY_FILTER_USE_KEY);
echo implode(',', array_keys($byKey)), "\n";

$sum = array_reduce($nums, fn (int $carry, int $n): int => $carry + $n, 10);
echo $sum, "\n";

// Coercion fallback mid-loop (string '20' -> int param) must stay exact.
$coerced = array_reduce([1, '20', 3], fn (int $carry, int $n): int => $carry + $n, 0);
echo $coerced, "\n";

// Exception mid-loop must unwind cleanly without corrupting native_loops stack.
try {
    array_map(function (int $n): int {
        if ($n === 3) {
            throw new RuntimeException('mid-loop');
        }
        return $n;
    }, $nums);
} catch (RuntimeException $e) {
    echo $e->getMessage(), "\n";
}

// Subsequent loop still works cleanly.
echo implode(',', array_map(fn (int $n): int => $n + 1, [7, 8])), "\n";
"#;

    let out = run_script("native-loops", SCRIPT);
    assert_eq!(
        out,
        "2,4,6,8,10\n11,22,33\n1,3:2,4\na,c\n25\n24\nmid-loop\n8,9\n"
    );
}

#[test]
fn try_catch_finally_functions_optimize_cleanly() {
    const SCRIPT: &str = r#"<?php
function compute(int $limit, bool $fail): string {
    $acc = 0;
    $log = '';
    try {
        for ($i = 1; $i <= $limit; $i++) {
            $acc += $i * 2;
            if ($fail && $i === 3) {
                throw new RuntimeException("stopped:$acc");
            }
        }
        $log .= "ok:$acc";
    } catch (RuntimeException $e) {
        $log .= 'caught:' . $e->getMessage();
    } finally {
        $log .= ":fin($acc)";
    }
    return $log;
}

echo compute(4, false), "\n";
echo compute(5, true), "\n";
"#;

    let out = run_script("try-opt", SCRIPT);
    assert_eq!(out, "ok:20:fin(20)\ncaught:stopped:12:fin(12)\n");
}
