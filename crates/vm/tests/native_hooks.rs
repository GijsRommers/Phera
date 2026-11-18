//! Native classes exercise the extension API through the same instructions as PHP classes.
//!
//! ```php
//! foreach ($nativeRange as $key => $value) { /* lazily computed */ }
//! ```

use std::sync::Arc;

use phera_extension_api::{Extension, Registry, testing::oracle};
use phera_vm::Vm;

mod native_hooks {
    pub mod counter;
    pub mod range;
    pub mod store;
}

fn registry() -> Registry {
    let mut registry = Registry::new();
    phera_ext_core::Core.register(&mut registry);
    phera_ext_json::Json.register(&mut registry);
    native_hooks::counter::register(&mut registry);
    native_hooks::range::register(&mut registry);
    native_hooks::store::register(&mut registry);
    registry
}

async fn compare(name: &str, model: &str, operations: &str) {
    let expected = oracle::run_php(name, &format!("<?php\n{model}\n{operations}"))
        .expect("PHP 8.5 oracle required");
    let unit = phera_compiler::compile(&format!("<?php\n{operations}"), "native-hooks.php")
        .expect("compile native hook test");
    let mut vm = Vm::new(Arc::new(registry()));
    let status = vm.run(Arc::new(unit)).await;
    let output = String::from_utf8(vm.output).expect("test output is UTF-8");
    assert_eq!(status, 0, "{output}");
    let actual: Vec<_> = output.lines().collect();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn cloning_comparing_projecting_and_constructing() {
    compare(
        "native-hook-counter",
        r#"
class EngineCounter {
    public int $value;
    public readonly int $stamp;
    public function __construct(int $value) { $this->value = $value; }
    public function read(): int { return $this->value; }
    public function stamp(): void { $this->stamp = 7; }
    public static function make(int $value): static { return new static($value); }
    public static function accept(EngineCounter $counter): int { return $counter->read(); }
}
"#,
        r#"
$a = new EngineCounter(5);
$b = clone $a;
$c = new EngineCounter(6);
var_dump($a === $b, $a == $b, [$a] == [$b], $a != $c);
var_dump($a < $c, $c <= $a, $c > $a, $a >= $b, $a <=> $c);
$a->stamp();
var_dump($a);
print_r($a);
echo var_export($a, true), "\n", json_encode($a), "\n";
var_dump((array) $a, get_object_vars($a));
foreach ($a as $key => $value) { echo "$key=$value\n"; }
class ChildCounter extends EngineCounter {
    public string $label = 'child';
}
$child = ChildCounter::make(8);
echo get_class($child), ' ', $child->label, ' ', EngineCounter::accept($child), "\n";
try { var_dump($child->stamp); } catch (Error $e) { echo $e->getMessage(), "\n"; }
$child->stamp();
try { $child->stamp = 10; } catch (Error $e) { echo $e->getMessage(), "\n"; }
var_dump($child->stamp);
class UninitializedCounter extends EngineCounter {
    public function __construct() {}
}
$missing = new UninitializedCounter;
try { $missing->read(); } catch (Error $e) { echo $e->getMessage(), "\n"; }
"#,
    )
    .await;
}

#[tokio::test]
async fn native_cursor_is_lazy_and_uses_all_iterator_consumers() {
    compare(
        "native-hook-range",
        r#"
class EngineRange implements IteratorAggregate {
    public function getIterator(): Iterator { return new RangeCursor; }
}
class RangeCursor implements Iterator {
    private int $position = 0;
    public function rewind(): void { $this->position = 0; }
    public function valid(): bool { return $this->position < 3; }
    public function current(): int { echo "value ", $this->position, "\n"; return $this->position + 3; }
    public function key(): int { return 10 + $this->position; }
    public function next(): void { ++$this->position; }
}
function uncloneable($range) { return new ReflectionClass($range); }
"#,
        r#"
$range = new EngineRange;
foreach ($range as $k => $v) { echo "$k $v\n"; break; }
var_dump(iterator_count($range), iterator_to_array($range), [...$range]);
function take($first,$second,$third) { var_dump($first,$second,$third); }
take(...$range);
$iterator = $range->getIterator();
var_dump($iterator->valid(), $iterator->key());
try { foreach ($range as &$item) {} } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { $copy = clone uncloneable($range); } catch (Error $e) {
    var_dump($e instanceof Error);
}
"#,
    ).await;
}

#[tokio::test]
async fn native_dimensions_and_count_match_array_object() {
    compare(
        "native-hook-store",
        "class EngineStore extends ArrayObject {}",
        r#"
$store = new EngineStore;
$store['name'] = 'sample';
$store[] = 5;
$store[0] += 2;
++$store[0];
$store['nested'] = ['a' => 7];
$store['nested']['a'] = 9;
var_dump($store[0], $store['nested']['a'], $store['name'], isset($store['name']));
var_dump(empty($store['name']), $store['missing'] ?? 11);
unset($store['name']);
var_dump(isset($store['name']), count($store), sizeof($store, COUNT_RECURSIVE));
"#,
    )
    .await;
}
