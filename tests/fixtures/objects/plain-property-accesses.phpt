--TEST--
One-step property reads, writes, unset, isset, ?? and empty by literal and variable names, without magic, hooks or readonly; magic, readonly and errors through the general path
--FILE--
<?php
#[AllowDynamicProperties] class H {
    public int $id; public ?string $name = null; public $u = 'u'; private $secret = 's'; public static $st = 1;
    public float $f = 0.0;
    public function readOwn(string $n) { return $this->$n; }
}
class M { private $hidden = 1; public function __get($n) { echo "  __get($n)\n"; return 9; } public function __isset($n) { echo "  __isset($n)\n"; return true; } public function __unset($n) { echo "  __unset($n)\n"; } }
class R { public readonly int $ro; public function __construct() { $this->ro = 1; } }
function show(string $label, callable $f) {
    try { $r = $f(); echo "$label: ", var_export($r, true), "\n"; } catch (Throwable $e) { echo "$label: ", get_class($e), ': ', $e->getMessage(), "\n"; }
}
$h = new H; $names = ['id', 'name', 'u', 'secret', 'dyn', '1', 'f'];
for ($i = 0; $i < 2; $i++) {
    foreach ($names as $n) {
        show("isset $n", fn () => isset($h->$n));
        show("?? $n", fn () => $h->$n ?? 'dflt');
        show("empty $n", fn () => empty($h->$n));
        show("write $n", function () use ($h, $n) { $h->$n = 5; return $h->$n; });
        show("read $n", fn () => $h->$n);
    }
}
show('write string to int', function () use ($h) { $h->id = '42'; return $h->id; });
show('write bad to int', fn () => $h->id = 'x');
show('write int to float', function () use ($h) { $n = 'f'; $h->$n = 3; return $h->f; });
show('assigned int to float, its result', function () use ($h) { $v = 1; $all = []; for ($k = 0; $k < 3; $k++) { $x = $h->f = $v; $all[] = $x; } return $all; });
class U { public $u = 1; public function __get($n) { return "__get($n)"; } }
show('unset declared, then magic read', function () { $o = new U; unset($o->u); return $o->u; });
class AV { public private(set) string $s = 'a'; public protected(set) string $t = 't'; }
$av = new AV;
for ($k = 0; $k < 2; $k++) { show('unset private(set)', function () use ($av) { unset($av->s); return 'done'; }); show('unset protected(set)', function () use ($av) { unset($av->t); return 'done'; }); }
show('read own private', fn () => $h->readOwn('secret'));
show('empty name', function () use ($h) { $n = ''; return $h->$n = 1; });
show('nul name', function () use ($h) { $n = "\0x"; return $h->$n; });
show('static as instance', function () use ($h) { $n = 'st'; return $h->$n = 2; });
foreach (['id', 'name', 'u', 'dyn'] as $n) { unset($h->$n); }
echo implode(',', array_keys(get_object_vars($h))), "\n";
show('read unset typed', fn () => $h->id);
show('read unset untyped', fn () => $h->u);
show('isset unset typed', fn () => isset($h->id));
$h->id = 7; $h->u = 'back'; $h->dyn = 'd';
echo implode(',', array_keys(get_object_vars($h))), "\n";
$ref = 1; $h->u = &$ref; $n = 'u'; $h->$n = 99; echo "ref=$ref\n"; unset($h->$n); echo "ref still $ref\n";
$m = new M;
show('magic isset', fn () => isset($m->hidden));
show('magic read', fn () => $m->hidden);
show('magic unset', function () use ($m) { unset($m->hidden); return 'done'; });
$r = new R;
show('readonly write', fn () => $r->ro = 2);
show('readonly unset', function () use ($r) { unset($r->ro); return 'done'; });
show('readonly read', fn () => $r->ro);

--EXPECTF--
isset id: false
?? id: 'dflt'
empty id: true
write id: 5
read id: 5
isset name: false
?? name: 'dflt'
empty name: true
write name: '5'
read name: '5'
isset u: true
?? u: 'u'
empty u: false
write u: 5
read u: 5
isset secret: false
?? secret: 'dflt'
empty secret: true
write secret: Error: Cannot access private property H::$secret
read secret: Error: Cannot access private property H::$secret
isset dyn: false
?? dyn: 'dflt'
empty dyn: true
write dyn: 5
read dyn: 5
isset 1: false
?? 1: 'dflt'
empty 1: true
write 1: 5
read 1: 5
isset f: true
?? f: 0.0
empty f: true
write f: 5.0
read f: 5.0
isset id: true
?? id: 5
empty id: false
write id: 5
read id: 5
isset name: true
?? name: '5'
empty name: false
write name: '5'
read name: '5'
isset u: true
?? u: 5
empty u: false
write u: 5
read u: 5
isset secret: false
?? secret: 'dflt'
empty secret: true
write secret: Error: Cannot access private property H::$secret
read secret: Error: Cannot access private property H::$secret
isset dyn: true
?? dyn: 5
empty dyn: false
write dyn: 5
read dyn: 5
isset 1: true
?? 1: 5
empty 1: false
write 1: 5
read 1: 5
isset f: true
?? f: 5.0
empty f: false
write f: 5.0
read f: 5.0
write string to int: 42
write bad to int: TypeError: Cannot assign string to property H::$id of type int
write int to float: 3.0
assigned int to float, its result: array (
  0 => 1.0,
  1 => 1.0,
  2 => 1.0,
)
unset declared, then magic read: '__get(u)'
unset private(set): Error: Cannot unset private(set) property AV::$s from global scope
unset protected(set): Error: Cannot unset protected(set) property AV::$t from global scope
unset private(set): Error: Cannot unset private(set) property AV::$s from global scope
unset protected(set): Error: Cannot unset protected(set) property AV::$t from global scope
read own private: 's'
empty name: 1
nul name: Error: Cannot access property starting with "\0"

Notice: Accessing static property H::$st as non static in %s on line 34
static as instance: 2
f,1,,st
read unset typed: Error: Typed property H::$id must not be accessed before initialization

Warning: Undefined property: H::$u in %s on line 38
read unset untyped: NULL
isset unset typed: false
id,u,f,1,,st,dyn
ref=99
ref still 99
  __isset(hidden)
magic isset: true
  __get(hidden)
magic read: 9
  __unset(hidden)
magic unset: 'done'
readonly write: Error: Cannot modify readonly property R::$ro
readonly unset: Error: Cannot unset readonly property R::$ro
readonly read: 1
