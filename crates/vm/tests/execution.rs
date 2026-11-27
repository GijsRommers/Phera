//! End-to-end register execution, bounded stacks, and amortised writes.

use std::sync::Arc;

use phera_extension_api::Registry;
use phera_vm::Vm;

/// Compile and run PHP in a fresh request with a configurable heap-frame bound.
async fn execute(source: &str, limit: usize) -> (i32, Vec<u8>) {
    let unit = Arc::new(phera_compiler::compile(source, "test.php").expect("valid PHP"));
    let mut vm = Vm::new(Arc::new(Registry::new()));
    vm.frame_limit = limit;
    let status = vm.run(unit).await;
    (status, vm.output)
}

/// Long expression chains must execute without consuming an unbounded Rust stack.
#[tokio::test(flavor = "current_thread")]
async fn deep_chains() {
    for (operator, term, expected) in [
        ("+", "1", "50000".to_owned()),
        (".", "'x'", "x".repeat(50_000)),
    ] {
        let source = format!(
            "<?php echo {};",
            std::iter::repeat_n(term, 50_000)
                .collect::<Vec<_>>()
                .join(operator)
        );
        let (status, output) = execute(&source, 10).await;
        assert_eq!(status, 0);
        assert_eq!(output, expected.as_bytes());
    }
}

/// PHP recursion uses heap frames even at depths far beyond a native stack.
#[tokio::test(flavor = "current_thread")]
async fn deep_php_recursion() {
    let(status,output)=execute("<?php function descend($n){if($n==0)return 42; return descend($n-1);} echo descend(200000);",250_000).await;
    assert_eq!(status, 0);
    assert_eq!(output, b"42");
}

/// Exceeding the configured frame limit must produce PHP output, not a panic.
#[tokio::test(flavor = "current_thread")]
async fn recursion_limit_is_php_error() {
    let (status, output) = execute("<?php function forever(){forever();} forever();", 256).await;
    assert_eq!(status, 255);
    assert!(String::from_utf8_lossy(&output).contains("Maximum call stack depth of 256 exceeded"));
}

/// An awaited native callback must resume the same isolated VM.
#[tokio::test(flavor = "current_thread")]
async fn async_native_callback_runs_on_same_vm() {
    use phera_extension_api::{
        Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Value,
    };
    fn callback<'a>(
        context: &'a mut dyn Context,
        arguments: Vec<Value>,
    ) -> LocalFuture<'a, NativeResult> {
        Box::pin(async move {
            tokio::task::yield_now().await;
            let callable = arguments.first().cloned().unwrap_or_default();
            context.call(&callable, vec![Value::Int(21)]).await
        })
    }
    const PARAMS: &[Param] = &[Param::new("fn", ParamType::Mixed)];
    let mut registry = Registry::new();
    registry.function(FunctionEntry::asynchronous("callback", PARAMS, callback));
    let unit = Arc::new(
        phera_compiler::compile(
            "<?php function twice($x){return $x*2;}echo callback('twice');",
            "test.php",
        )
        .expect("valid PHP"),
    );
    let mut vm = Vm::new(Arc::new(registry));
    assert_eq!(vm.run(unit).await, 0);
    assert_eq!(vm.output, b"42");
}

/// Repeated PHP concatenation retains amortized string growth.
#[tokio::test(flavor = "current_thread")]
async fn million_string_appends() {
    let (status, output) = execute(
        "<?php $text='';for($index=0;$index<1000000;$index++){$text.='x';}echo $text;",
        10,
    )
    .await;
    assert_eq!(status, 0);
    assert_eq!(output.len(), 1_000_000);
    assert!(output.iter().all(|byte| *byte == b'x'));
}

/// Appending repeatedly must not retain a temporary COW owner.
#[tokio::test(flavor = "current_thread")]
async fn million_array_pushes() {
    let (status,output) = execute("<?php $values=[];for($index=0;$index<1000000;$index++){$values[]=$index;}echo $values[999999];",10).await;
    assert_eq!((status, output), (0, b"999999".to_vec()));
}

/// Nested reconstruction must preserve every element during repeated writes.
#[tokio::test(flavor = "current_thread")]
async fn million_nested_array_writes() {
    let (status,output) = execute("<?php $grid=[];for($row=0;$row<1000;$row++){for($column=0;$column<1000;$column++){$grid[$row][$column]=$row+$column;}}echo $grid[999][999];",10).await;
    assert_eq!((status, output), (0, b"1998".to_vec()));
}

/// PHP's argument introspection sees assignments to the original parameter slot.
#[tokio::test(flavor = "current_thread")]
async fn observed_arguments_follow_parameter_assignment() {
    let (status, output) = execute(
        "<?php function inspect($value){$value=9;echo func_num_args(),':',func_get_arg(0),':';$args=func_get_args();echo $args[0];}inspect(1);",
        10,
    ).await;
    assert_eq!((status, output), (0, b"1:9:9".to_vec()));
}

/// A cached namespace fallback must observe a later definition in that namespace.
#[tokio::test(flavor = "current_thread")]
async fn constant_cache_invalidates_namespace_fallbacks() {
    let (status, output) = execute(
        "<?php namespace A;define('K',1);echo K;define('A\\K',2);echo K;",
        10,
    )
    .await;
    assert_eq!((status, output), (0, b"12".to_vec()));
}

/// A long linked list of objects, with and without destructors, freed in the
/// middle of the script and at its end, is released without Rust recursion:
/// 300,000 nodes are far more than a test thread's stack could recurse
/// through. (PHP itself overflows its C stack freeing a list of a million,
/// so this is checked here rather than against PHP's output.)
#[tokio::test(flavor = "current_thread")]
async fn a_long_object_list_is_freed_without_recursion() {
    let source = r#"<?php
class Plain { public $next; }
class Counted { public $next; public static $destructed = 0; function __destruct() { self::$destructed++; } }
foreach (['Plain', 'Counted'] as $class) {
    $head = null;
    for ($i = 0; $i < 300000; $i++) { $node = new $class; $node->next = $head; $head = $node; }
    $node = null;
    $head = null;
    echo $class, " ";
}
echo Counted::$destructed, " ";
for ($i = 0; $i < 300000; $i++) { $node = new Counted; $node->next = $head; $head = $node; }
$node = null;
echo "end";
"#;
    let (status, output) = execute(source, 10).await;
    assert_eq!(status, 0);
    assert_eq!(output, b"Plain Counted 300000 end");
}

/// Scope- and frame-inspecting intrinsics cannot be invoked dynamically in PHP 8.5,
/// preserving static soundness of local reference and by-name analysis.
#[tokio::test(flavor = "current_thread")]
async fn dynamic_scope_intrinsics_match_php_8_5() {
    let source = r#"<?php
function run_case(string $label, callable $fn): void {
    try {
        $fn();
        echo "$label:OK\n";
    } catch (Throwable $e) {
        echo "$label:" . get_class($e) . ":" . $e->getMessage() . "\n";
    }
}

// Global scope checks: arg errors happen before dynamic error; func_* global checks precede dynamic error.
run_case("g-ext-bad", function() { });
$f = "extract";
try { $f(123); } catch (Throwable $e) { echo "g-ext-123:" . get_class($e) . ":" . $e->getMessage() . "\n"; }
try { $f(["a" => 1]); } catch (Throwable $e) { echo "g-ext-ok:" . get_class($e) . ":" . $e->getMessage() . "\n"; }
$f = "compact";
try { $f(); } catch (Throwable $e) { echo "g-cmp-0:" . get_class($e) . ":" . $e->getMessage() . "\n"; }
try { $f("x"); } catch (Throwable $e) { echo "g-cmp-ok:" . get_class($e) . ":" . $e->getMessage() . "\n"; }
$f = "get_defined_vars";
try { $f(1); } catch (Throwable $e) { echo "g-gdv-1:" . get_class($e) . ":" . $e->getMessage() . "\n"; }
try { $f(); } catch (Throwable $e) { echo "g-gdv-ok:" . get_class($e) . ":" . $e->getMessage() . "\n"; }
$f = "func_num_args";
try { $f(); } catch (Throwable $e) { echo "g-fna:" . get_class($e) . ":" . $e->getMessage() . "\n"; }
$f = "func_get_arg";
try { $f(-1); } catch (Throwable $e) { echo "g-fga-neg:" . get_class($e) . ":" . $e->getMessage() . "\n"; }
try { $f(0); } catch (Throwable $e) { echo "g-fga-0:" . get_class($e) . ":" . $e->getMessage() . "\n"; }

function inside($x): void {
    foreach (["extract", "compact", "get_defined_vars", "func_num_args", "func_get_args", "func_get_arg"] as $fn) {
        foreach (["var", "cuf", "cufa", "fcc", "inv", "cfc"] as $mode) {
            try {
                if ($mode === "var") {
                    $f = $fn;
                    if ($fn === "extract") $f(["k" => 1]);
                    elseif ($fn === "compact") $f("x");
                    elseif ($fn === "func_get_arg") $f(0);
                    else $f();
                } elseif ($mode === "cuf") {
                    if ($fn === "extract") call_user_func($fn, ["k" => 1]);
                    elseif ($fn === "compact") call_user_func($fn, "x");
                    elseif ($fn === "func_get_arg") call_user_func($fn, 0);
                    else call_user_func($fn);
                } elseif ($mode === "cufa") {
                    if ($fn === "extract") call_user_func_array($fn, [["k" => 1]]);
                    elseif ($fn === "compact") call_user_func_array($fn, ["x"]);
                    elseif ($fn === "func_get_arg") call_user_func_array($fn, [0]);
                    else call_user_func_array($fn, []);
                } elseif ($mode === "fcc") {
                    $c = $fn(...);
                    if ($fn === "extract") $c(["k" => 1]);
                    elseif ($fn === "compact") $c("x");
                    elseif ($fn === "func_get_arg") $c(0);
                    else $c();
                } elseif ($mode === "inv") {
                    $c = $fn(...);
                    if ($fn === "extract") $c->__invoke(["k" => 1]);
                    elseif ($fn === "compact") $c->__invoke("x");
                    elseif ($fn === "func_get_arg") $c->__invoke(0);
                    else $c->__invoke();
                } else {
                    $c = Closure::fromCallable($fn);
                    if ($fn === "extract") $c(["k" => 1]);
                    elseif ($fn === "compact") $c("x");
                    elseif ($fn === "func_get_arg") $c(0);
                    else $c();
                }
                echo "$fn-$mode:OK\n";
            } catch (Throwable $e) {
                echo "$fn-$mode:" . get_class($e) . ":" . $e->getMessage() . "\n";
            }
        }
    }
    // Direct calls still work normally, including EXTR_REFS.
    $src = ["extracted" => 7];
    $n = extract($src, EXTR_REFS);
    $extracted = 42;
    $c = compact("x", "extracted");
    $d = get_defined_vars();
    echo "direct:$n:" . $src["extracted"] . ":" . $c["x"] . ":" . $d["extracted"] . ":" . func_num_args() . ":" . func_get_arg(0) . ":" . func_get_args()[0] . "\n";
}
inside(99);
"#;
    let (status, output) = {
        use phera_extension_api::Extension;
        let mut registry = Registry::new();
        phera_ext_core::Core.register(&mut registry);
        let unit = Arc::new(phera_compiler::compile(source, "test.php").expect("valid PHP"));
        let mut vm = Vm::new(Arc::new(registry));
        vm.frame_limit = 20;
        let status = vm.run(unit).await;
        (status, vm.output)
    };
    assert_eq!(
        status,
        0,
        "stderr/out: {}",
        String::from_utf8_lossy(&output)
    );
    let out = String::from_utf8(output).expect("utf-8 output");
    assert!(
        out.contains(
            "g-ext-123:TypeError:extract(): Argument #1 ($array) must be of type array, int given\n"
        ),
        "{out}"
    );
    assert!(
        out.contains("g-ext-ok:Error:Cannot call extract() dynamically\n"),
        "{out}"
    );
    assert!(
        out.contains("g-cmp-0:ArgumentCountError:compact() expects at least 1 argument, 0 given\n"),
        "{out}"
    );
    assert!(
        out.contains("g-cmp-ok:Error:Cannot call compact() dynamically\n"),
        "{out}"
    );
    assert!(
        out.contains(
            "g-gdv-1:ArgumentCountError:get_defined_vars() expects exactly 0 arguments, 1 given\n"
        ),
        "{out}"
    );
    assert!(
        out.contains("g-gdv-ok:Error:Cannot call get_defined_vars() dynamically\n"),
        "{out}"
    );
    assert!(
        out.contains("g-fna:Error:func_num_args() must be called from a function context\n"),
        "{out}"
    );
    assert!(out.contains("g-fga-neg:ValueError:func_get_arg(): Argument #1 ($position) must be greater than or equal to 0\n"), "{out}");
    assert!(
        out.contains("g-fga-0:Error:func_get_arg() cannot be called from the global scope\n"),
        "{out}"
    );
    for fn_name in [
        "extract",
        "compact",
        "get_defined_vars",
        "func_num_args",
        "func_get_args",
        "func_get_arg",
    ] {
        for mode in ["var", "cuf", "cufa", "fcc", "inv", "cfc"] {
            let expected = format!("{fn_name}-{mode}:Error:Cannot call {fn_name}() dynamically\n");
            assert!(out.contains(&expected), "missing {expected:?} in:\n{out}");
        }
    }
    assert!(out.contains("direct:1:42:99:42:1:99:99\n"), "{out}");
}
