--TEST--
Reading one key of `(array) $object` (`((array) $o)[$key] ?? null`, `isset`, a plain read): mangled names, a key that is not the mangled name, uninitialized and hooked properties, references, evaluation order of the cast and the key, an accessor that keeps the key in a property, magic and native classes, lazy objects
--FILE--
<?php
set_error_handler(function ($no, $msg) { echo "  [$msg]\n"; return true; });
class Base { private $secret = 'base-secret'; protected $label = 'base-label'; public $plain = 'base-plain'; }
class Child extends Base {
    private $secret = 'child-secret';
    public int $typed;
    public ?int $nullable = null;
    public $ref = 'r';
    public $hooked { get => 'hook:' . $this->hooked; set => $this->hooked = $value; }
    public function __construct() { $this->hooked = 'stored'; }
}
class Magic { public $real = 'real'; function __get($n) { echo "__get $n\n"; return 'magic'; } function __isset($n) { echo "__isset $n\n"; return true; } }
class Accessor {
    public function __construct(private string $key) {}
    public function read(object $o): mixed { return ((array) $o)[$this->key] ?? null; }
    public function has(object $o): bool { return isset(((array) $o)[$this->key]); }
}
class Mutator {
    public $x = 'before';
    private $k = 'x';
    function __get($name) { $this->x = 'changed by __get'; return 'x'; }
}
class Counter { public $n = 0; }

function show(string $label, callable $f): void {
    try { echo $label, ': ', var_export($f(), true), "\n"; }
    catch (Throwable $e) { echo $label, ': ', get_class($e), ': ', $e->getMessage(), "\n"; }
}
function bump(Counter $c, string $key): string { $c->n++; return $key; }

$c = new Child;
for ($i = 0; $i < 2; $i++) {
    echo "-- round $i\n";
    show('public', fn () => ((array) $c)['plain'] ?? 'missing');
    show('protected, mangled', fn () => ((array) $c)["\0*\0label"] ?? 'missing');
    show('protected, plain name', fn () => ((array) $c)['label'] ?? 'missing');
    show('private of the child', fn () => ((array) $c)["\0Child\0secret"] ?? 'missing');
    show('private of the parent', fn () => ((array) $c)["\0Base\0secret"] ?? 'missing');
    show('private, plain name', fn () => ((array) $c)['secret'] ?? 'missing');
    show('private, wrong class', fn () => ((array) $c)["\0Other\0secret"] ?? 'missing');
    show('uninitialized typed', fn () => ((array) $c)['typed'] ?? 'missing');
    show('uninitialized typed isset', fn () => isset(((array) $c)['typed']));
    show('null value with ??', fn () => ((array) $c)['nullable'] ?? 'default');
    show('null value with isset', fn () => isset(((array) $c)['nullable']));
    show('null value, key exists', fn () => array_key_exists('nullable', (array) $c));
    show('hooked property, backing value', fn () => ((array) $c)['hooked'] ?? 'missing');
    show('missing key with ??', fn () => ((array) $c)['nothing'] ?? 'default');
    show('missing key with isset', fn () => isset(((array) $c)['nothing']));
    show('missing key, plain read', fn () => ((array) $c)['nothing']);
    show('missing mangled key, plain read', fn () => ((array) $c)["\0Child\0nothing"]);
    show('int key', fn () => ((array) $c)[0] ?? 'missing');
    show('float key', fn () => ((array) $c)[1.5] ?? 'missing');
    show('null key', fn () => ((array) $c)[null] ?? 'missing');
    show('bool key', fn () => ((array) $c)[true] ?? 'missing');
    show('array key', fn () => ((array) $c)[[]] ?? 'missing');
    show('present, plain read', fn () => ((array) $c)['plain']);
}
echo "-- references\n";
$o = new Counter; $r = &$o->n; $r = 7;
show('property that is a reference', fn () => ((array) $o)['n'] ?? 'missing');
show('same, isset', fn () => isset(((array) $o)['n']));
unset($r);
$o2 = new Counter; $a = (array) $o2;
show('cast copy is not the property', function () use ($o2) { $a = (array) $o2; $a['n'] = 99; return $o2->n; });
echo "-- order of evaluation\n";
$counter = new Counter; $target = new Counter;
show('the key expression runs once, after the cast', function () use ($target, $counter) { $v = ((array) $target)[bump($counter, 'n')] ?? 'missing'; return [$v, $counter->n]; });
show('the cast sees the object before the key expression changes it', function () {
    $o = new Counter;
    $change = function () use ($o) { $o->n = 'changed'; return 'n'; };
    return ((array) $o)[$change()] ?? 'missing';
});
show('the cast is evaluated before the key expression', function () {
    $log = [];
    $make = function () use (&$log) { $log[] = 'object'; return new Counter; };
    $key = function () use (&$log) { $log[] = 'key'; return 'n'; };
    $v = ((array) $make())[$key()] ?? 'missing';
    return [$v, $log];
});
show('a key from a __get that changes the object', function () {
    $m = new Mutator;
    $get = new class($m) { function __construct(public Mutator $m) {} function __get($n) { $this->m->x = 'changed by __get'; return 'x'; } };
    return ((array) $m)[$get->anything] ?? 'missing';
});
show('undefined variable as the key', function () {
    $o = new Counter;
    return ((array) $o)[$undefined] ?? 'missing';
});
echo "-- accessor with the key in a property\n";
$acc = new Accessor('plain'); $accPrivate = new Accessor("\0Child\0secret"); $accMissing = new Accessor('nope');
for ($i = 0; $i < 2; $i++) {
    show('accessor public', fn () => $acc->read($c));
    show('accessor private', fn () => $accPrivate->read($c));
    show('accessor missing', fn () => $accMissing->read($c));
    show('accessor isset', fn () => [$acc->has($c), $accMissing->has($c)]);
    show('accessor on another object', fn () => $acc->read(new Counter));
    show('accessor on an array', fn () => $acc->read(new ArrayObject(['plain' => 'in storage'])));
}
echo "-- other kinds of operand\n";
show('null', fn () => ((array) null)['a'] ?? 'missing');
show('string', fn () => ((array) 'abc')[0] ?? 'missing');
show('int', fn () => ((array) 5)[0] ?? 'missing');
show('array', fn () => ((array) ['a' => 1])['a'] ?? 'missing');
show('stdClass', function () { $s = new stdClass; $s->a = 1; $s->{'5'} = 'five'; return [((array) $s)['a'] ?? 'm', ((array) $s)[5] ?? 'm', ((array) $s)['5'] ?? 'm']; });
show('dynamic property', function () { $d = new Counter; $d->dyn = 'd'; return ((array) $d)['dyn'] ?? 'missing'; });
show('integer-like name', function () { $d = (object) [5 => 'five']; return [((array) $d)[5] ?? 'm', ((array) $d)['5'] ?? 'm']; });
show('ArrayObject', fn () => ((array) new ArrayObject(['k' => 'v']))['k'] ?? 'missing');
show('ArrayObject own property', function () { $a = new ArrayObject(['k' => 'v']); $a->prop = 'p'; return ((array) $a)['prop'] ?? 'missing'; });
show('ArrayIterator', fn () => ((array) new ArrayIterator(['k' => 'v']))['k'] ?? 'missing');
show('closure', fn () => ((array) function () {})[0] instanceof Closure);
show('object with __get', fn () => ((array) new Magic)['real'] ?? 'missing');
show('object with __get, missing key', fn () => ((array) new Magic)['virtual'] ?? 'missing');
show('object with __isset, isset', fn () => isset(((array) new Magic)['virtual']));
show('DateTime', fn () => array_key_exists('date', (array) new DateTime('2020-01-02 UTC')) ? 'has date' : 'no date');
enum Suit: string { case Hearts = 'H'; }
show('enum', fn () => ((array) Suit::Hearts)['value'] ?? 'missing');
echo "-- lazy objects\n";
$rc = new ReflectionClass(Child::class);
$ghost = $rc->newLazyGhost(function ($o) { echo "init ghost\n"; $o->typed = 1; });
show('lazy ghost', fn () => ((array) $ghost)['plain'] ?? 'missing');
show('lazy ghost still lazy', fn () => $rc->isUninitializedLazyObject($ghost));
$proxy = $rc->newLazyProxy(function ($o) { echo "init proxy\n"; $c = new Child; $c->plain = 'from the real one'; return $c; });
show('lazy proxy', fn () => ((array) $proxy)['plain'] ?? 'missing');
show('lazy proxy still lazy', fn () => $rc->isUninitializedLazyObject($proxy));
show('initialized ghost', function () use ($rc) { $g = $rc->newLazyGhost(function ($o) { $o->typed = 1; }); $g->typed; return ((array) $g)['typed'] ?? 'missing'; });

--EXPECT--
-- round 0
public: 'base-plain'
protected, mangled: 'base-label'
protected, plain name: 'missing'
private of the child: 'child-secret'
private of the parent: 'base-secret'
private, plain name: 'missing'
private, wrong class: 'missing'
uninitialized typed: 'missing'
uninitialized typed isset: false
null value with ??: 'default'
null value with isset: false
null value, key exists: true
hooked property, backing value: 'stored'
missing key with ??: 'default'
missing key with isset: false
missing key, plain read:   [Undefined array key "nothing"]
NULL
missing mangled key, plain read:   [Undefined array key ""]
NULL
int key: 'missing'
float key:   [Implicit conversion from float 1.5 to int loses precision]
'missing'
null key:   [Using null as an array offset is deprecated, use an empty string instead]
'missing'
bool key: 'missing'
array key: array key: TypeError: Cannot access offset of type array on array
present, plain read: 'base-plain'
-- round 1
public: 'base-plain'
protected, mangled: 'base-label'
protected, plain name: 'missing'
private of the child: 'child-secret'
private of the parent: 'base-secret'
private, plain name: 'missing'
private, wrong class: 'missing'
uninitialized typed: 'missing'
uninitialized typed isset: false
null value with ??: 'default'
null value with isset: false
null value, key exists: true
hooked property, backing value: 'stored'
missing key with ??: 'default'
missing key with isset: false
missing key, plain read:   [Undefined array key "nothing"]
NULL
missing mangled key, plain read:   [Undefined array key ""]
NULL
int key: 'missing'
float key:   [Implicit conversion from float 1.5 to int loses precision]
'missing'
null key:   [Using null as an array offset is deprecated, use an empty string instead]
'missing'
bool key: 'missing'
array key: array key: TypeError: Cannot access offset of type array on array
present, plain read: 'base-plain'
-- references
property that is a reference: 7
same, isset: true
cast copy is not the property: 0
-- order of evaluation
the key expression runs once, after the cast: array (
  0 => 0,
  1 => 1,
)
the cast sees the object before the key expression changes it: 0
the cast is evaluated before the key expression: array (
  0 => 0,
  1 => 
  array (
    0 => 'object',
    1 => 'key',
  ),
)
a key from a __get that changes the object: 'before'
undefined variable as the key:   [Undefined variable $undefined]
  [Using null as an array offset is deprecated, use an empty string instead]
'missing'
-- accessor with the key in a property
accessor public: 'base-plain'
accessor private: 'child-secret'
accessor missing: NULL
accessor isset: array (
  0 => true,
  1 => false,
)
accessor on another object: NULL
accessor on an array: 'in storage'
accessor public: 'base-plain'
accessor private: 'child-secret'
accessor missing: NULL
accessor isset: array (
  0 => true,
  1 => false,
)
accessor on another object: NULL
accessor on an array: 'in storage'
-- other kinds of operand
null: 'missing'
string: 'abc'
int: 5
array: 1
stdClass: array (
  0 => 1,
  1 => 'five',
  2 => 'five',
)
dynamic property:   [Creation of dynamic property Counter::$dyn is deprecated]
'd'
integer-like name: array (
  0 => 'five',
  1 => 'five',
)
ArrayObject: 'v'
ArrayObject own property:   [Creation of dynamic property ArrayObject::$prop is deprecated]
'missing'
ArrayIterator: 'v'
closure: true
object with __get: 'real'
object with __get, missing key: 'missing'
object with __isset, isset: false
DateTime: 'has date'
enum: 'H'
-- lazy objects
lazy ghost: 'missing'
lazy ghost still lazy: true
lazy proxy: 'missing'
lazy proxy still lazy: true
initialized ghost: 1
