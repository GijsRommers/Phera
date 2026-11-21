//! Method sites against PHP in the two cases the review of the call-site
//! train found (Fable, #173): a native method whose later argument the quick
//! send refuses (an undefined local, a by-reference or variadic parameter),
//! and a trait method composed into two classes, whose copies share their
//! site records but not their scope (both orders).
//!
//! The rows are read with `fetchAll`, not `foreach` over the statement:
//! that `foreach` yields strings on main too, a separate bug.

use phera_runtime::Runtime;

const NATIVE_ARGUMENTS: &str = r#"<?php
// Native methods through a site, where a later argument is refused by the quick send.
$list = new ArrayObject([1, 2, 3]);
$i = 1;
$list->offsetSet($i, $undefined);
var_dump($list->getArrayCopy());
$k = 5; $v = 'five';
$list->offsetSet($k, $v);
$r = &$v;
$list->offsetSet($i, $r);
var_dump($list->getArrayCopy());

$pdo = new PDO('sqlite::memory:');
$pdo->exec('create table t (a integer, b text)');
$stmt = $pdo->prepare('insert into t values (:a, :b)');
$name = ':a'; $a = 7; $nameb = ':b'; $b = 'seven';
$stmt->bindParam($name, $a);
$stmt->bindParam($nameb, $b);
$a = 8; $b = 'eight';
$stmt->execute();
$type = PDO::PARAM_INT;
$stmt->bindParam($name, $a, $type);
$stmt->bindValue($nameb, $b, PDO::PARAM_STR);
$a = 9;
$stmt->execute();
var_dump($pdo->query('select * from t')->fetchAll(PDO::FETCH_NUM));
$q = $pdo->query('select a, b from t');
$mode = PDO::FETCH_NUM;
$q->setFetchMode($mode);
var_dump($q->fetch());
$q = $pdo->query('select a, b from t');
$col = 1;
$q->setFetchMode(PDO::FETCH_COLUMN, $col);
var_dump($q->fetchAll());

$d = new DateTime('2026-01-02 03:04:05', new DateTimeZone('UTC'));
$h = 10; $m = 20;
$d->setTime($h, $m);
echo $d->format('c'), "\n";
$d->setTime($h, $undef2);
echo $d->format('c'), "\n";
$fmt = 'Y';
echo $d->format($fmt), ' ', $d->format($nope ?? 'm'), "\n";
try { $d->setTime($h); } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $d->setTime($h, $m, $h, $m, $h); } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $d->setTime($fmt, $m); } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }

$s = new SplObjectStorage();
$o1 = new stdClass; $info = 'data';
$s->attach($o1, $info);
$s->offsetSet($o1, $undef3);
var_dump($s[$o1]);
$fixed = new SplFixedArray(3);
$zero = 0; $val = 'z';
$fixed->offsetSet($zero, $val);
try { $fixed->offsetSet($k, $val); } catch (RuntimeException $e) { echo $e->getMessage(), "\n"; }
var_dump($fixed->toArray());
// A reflection call with arguments (variadic native).
class C { function m($a, $b = 2, ...$rest) { return func_get_args(); } }
$rm = new ReflectionMethod('C', 'm'); $c = new C; $x = 1; $y = 2; $z = 3;
var_dump($rm->invoke($c, $x, $y, $z));
$args = [$x, $y];
var_dump($rm->invokeArgs($c, $args));
$cl = function (...$a) { return $a; };
var_dump($cl->call($c, $x, $y));
var_dump($cl->__invoke($x, $y));
$ao = new ArrayIterator([3, 1, 2]);
$cmp = fn($a, $b) => $a <=> $b;
$ao->uasort($cmp);
var_dump(iterator_to_array($ao));
"#;

const TRAIT_SCOPES: &str = r#"<?php
// A trait method composed into two classes: the same code, two scopes.
trait Runs {
    public function run($o) { return $o->id(); }
    public function tryRun($o) { try { return $o->secret(); } catch (Error $e) { return $e->getMessage(); } }
    public function tryGuarded($o) { try { return $o->guarded(); } catch (Error $e) { return $e->getMessage(); } }
}
class P { use Runs; private function id() { return 'P private'; } private function secret() { return 'P secret'; } protected function guarded() { return 'P guarded'; } }
class C extends P { public function id() { return 'C public'; } }
class Q { use Runs; }
$p = new P; $c = new C; $q = new Q;
for ($round = 0; $round < 3; $round++) {
    echo $p->run($c), ' | ', $q->run($c), ' | ', $p->tryRun($p), ' | ', $q->tryRun($p), ' | ', $p->tryGuarded($p), ' | ', $q->tryGuarded($p), "\n";
}
for ($round = 0; $round < 3; $round++) {
    echo $q->run($c), ' | ', $p->run($c), ' | ', $q->tryRun($p), ' | ', $p->tryRun($p), ' | ', $q->tryGuarded($p), ' | ', $p->tryGuarded($p), "\n";
}
// The same through a static closure bound to scopes, and Closure::call.
$f = function ($o) { try { return $o->secret(); } catch (Error $e) { return $e->getMessage(); } };
$inP = Closure::bind($f, null, P::class); $inQ = Closure::bind($f, null, Q::class);
for ($round = 0; $round < 2; $round++) { echo $inP($p), ' | ', $inQ($p), ' | ', $f($p), "\n"; }
// The reverse order, and a static closure bound to each scope.
for ($round = 0; $round < 2; $round++) { echo $inQ($p), ' | ', $f($p), ' | ', $inP($p), "\n"; }
$s = static function ($o) { try { return $o->secret(); } catch (Error $e) { return $e->getMessage(); } };
$sP = Closure::bind($s, null, P::class); $sQ = Closure::bind($s, null, Q::class);
for ($round = 0; $round < 2; $round++) { echo $sQ($p), ' | ', $sP($p), ' | ', $s($p), "\n"; }
// Inherited method called on behalf of a child: scope is the declaring class.
class Base { public function viaBase($o) { return $o->hidden(); } private function hidden() { return 'Base hidden'; } }
class Child extends Base { public function hidden() { return 'Child hidden'; } }
$b = new Base; $ch = new Child;
for ($round = 0; $round < 2; $round++) { echo $b->viaBase($ch), ' | ', $ch->viaBase($ch), ' | ', $ch->viaBase($b), ' | ', $ch->hidden(), "\n"; }
"#;

/// The script's path appears in warnings; PHP's and ours differ.
fn without_paths(line: &str) -> String {
    match (line.find(" in /"), line.rfind(" on line ")) {
        (Some(start), Some(end)) if start < end => {
            format!("{} in FILE{}", &line[..start], &line[end..])
        }
        _ => line.to_owned(),
    }
}

fn same_as_php(name: &str, script: &str) {
    let Some(expected) = phera_extension_api::testing::oracle::run_php(name, script) else {
        eprintln!("SKIP: PHP 8.5 is not available");
        return;
    };
    assert!(expected.len() > 5, "PHP printed {expected:?}");
    let dir = std::env::temp_dir().join(format!("phera-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("test directory");
    let path = dir.join("index.php");
    std::fs::write(&path, script).expect("script");
    let result = Runtime::new()
        .run_file_blocking(&path, &[], None)
        .expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    let output = String::from_utf8_lossy(&result.output);
    let got: Vec<String> = output.lines().map(without_paths).collect();
    let expected: Vec<String> = expected.iter().map(|line| without_paths(line)).collect();
    assert_eq!(got, expected);
}

#[test]
fn native_method_arguments_the_quick_send_refuses() {
    same_as_php("site-native-arguments", NATIVE_ARGUMENTS);
}

#[test]
fn trait_method_copies_keep_their_own_scope() {
    same_as_php("site-trait-scopes", TRAIT_SCOPES);
}
