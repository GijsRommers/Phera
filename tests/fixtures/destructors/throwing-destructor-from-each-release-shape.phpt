--TEST--
A destructor that throws from each releasing shape, while a long-lived object with a destructor exists
--FILE--
<?php
// A destructor that throws, from each releasing shape.
class T {
    public function __construct(public string $name) {}
    public function __toString(): string { return $this->name; }
    public function __destruct() { echo "  ~{$this->name}\n"; throw new Exception("from {$this->name}"); }
}
class Holder { public $p; }
$service = new T('service-never-thrown');
function run(string $label, callable $f) {
    try { $f(); echo "  no exception\n"; } catch (Exception $e) { echo "  caught: ", $e->getMessage(), "\n"; }
}
run('overwrite', function () { $a = new T('overwrite'); $a = 1; echo "  not reached\n"; });
run('unset', function () { $a = new T('unset'); unset($a); echo "  not reached\n"; });
run('element', function () { $arr = [new T('element')]; $arr[0] = 2; echo "  not reached\n"; });
run('property', function () { $h = new Holder; $h->p = new T('property'); $h->p = null; echo "  not reached\n"; });
run('compound', function () { $arr = ['n' => new T('compound')]; $arr['n'] .= 'x'; echo "  not reached\n"; });
run('return', function () { $x = new T('return'); return 1; });
run('foreach', function () { foreach ([new T('foreach')] as $v) {} unset($v); echo "  not reached\n"; });
run('two in a row', function () { $a = new T('first'); $b = new T('second'); $a = $b = null; echo "  not reached\n"; });
$service = null;

--EXPECTF--
  ~overwrite
  caught: from overwrite
  ~unset
  caught: from unset
  ~element
  caught: from element
  ~property
  caught: from property
  ~compound
  caught: from compound
  ~return
  caught: from return
  ~foreach
  caught: from foreach
  ~second
  ~first
  caught: from first
  ~service-never-thrown

Fatal error: Uncaught Exception: from service-never-thrown in %s:6
Stack trace:
#0 %s(21): T->__destruct()
#1 {main}
  thrown in %s on line 6
