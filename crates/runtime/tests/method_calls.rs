//! Method calls whose arguments are locals or literals run as one
//! `CallMethod`; the rest keep `InitMethodCall`, the sends and `Call`.
//! Either way a program must see what PHP shows: the method is found
//! before an argument with an effect runs, `__call`, references,
//! coercions, visibility, named and spread arguments, and the errors.

use phera_runtime::Runtime;

const SCRIPT: &str = r#"<?php
class Shape {
    protected int $base = 100;
    public function add(int $x, int $y = 10): int { return $x + $y; }
    public function half(float $f): float { return $f / 2; }
    public function bump(int &$x): void { $x++; }
    public function count(...$items): int { return count($items); }
    public function own(): string { return $this->secret(); }
    private function secret(): string { return 'shape secret'; }
    protected function guarded(): int { return $this->base; }
    public function same(Shape $other): string { return get_class($other); }
    public function __call($name, $arguments) { return "__call $name/" . count($arguments); }
}
class Square extends Shape {
    private function secret(): string { return 'square secret'; }
    public function viaParent(): int { return $this->guarded(); }
}
function effect($value) { echo "effect $value\n"; return $value; }
function thrower() { throw new RuntimeException('argument threw'); }

$shape = new Shape(); $square = new Square(); $i = 3;
// One instruction: locals and literals.
echo $shape->add($i), ' ', $shape->add($i, 4), ' ', $shape->add(1, 2), "\n";
echo $shape->half($i), "\n";                       // int to float on the way in
echo $square->own(), ' ', $square->viaParent(), "\n"; // private of the scope, protected
echo $shape->same($square), "\n";
$shape->bump($i); echo "bumped $i\n";              // a by-reference parameter
echo $shape->missing($i, 2), "\n";                 // __call
try { echo $shape->add('three'); } catch (TypeError $e) { echo 'TypeError: ', strtok($e->getMessage(), ','), "\n"; }
$nothing = null;
try { $nothing->add($i); } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { (new stdClass)->absent($undefined); } catch (Error $e) { echo $e->getMessage(), "\n"; }
echo $shape->add($undefinedToo ?? 5), "\n";
// The general sequence: the method is found before these arguments run.
try { (new stdClass)->absent(effect('first')); } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { (new stdClass)->absent(thrower()); } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { $shape->add(thrower()); } catch (RuntimeException $e) { echo $e->getMessage(), "\n"; }
echo $shape->add(effect(1), effect(2)), "\n";
function make() { echo "make\n"; return new Shape(); }
echo make()->add($i), "\n";                         // a receiver that is a call
echo $shape->add(y: 1, x: 2), "\n";                // named arguments
$arguments = [5, 6];
echo $shape->add(...$arguments), ' ', $shape->count(...$arguments), "\n";
try { echo $shape->add($i, thrower()); } catch (RuntimeException $e) { echo 'in try: ', $e->getMessage(), "\n"; }
function insideTry(Shape $shape, int $i) {
    try { return $shape->add($i, 'x'); } catch (TypeError $e) { return 'caught in a function'; }
}
echo insideTry($shape, $i), "\n";
function recursive(Shape $shape, int $depth): int { return $depth === 0 ? 0 : 1 + recursive($shape, $shape->add($depth, -1)); }
echo recursive($shape, 3), "\n";
"#;

#[test]
fn method_calls_behave_as_in_php() {
    let Some(expected) = phera_extension_api::testing::oracle::run_php("method-calls", SCRIPT)
    else {
        eprintln!("SKIP: PHP 8.5 is not available");
        return;
    };
    assert!(expected.len() > 20, "PHP printed {expected:?}");
    let dir = std::env::temp_dir().join(format!("phera-method-calls-test-{}", std::process::id()));
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
