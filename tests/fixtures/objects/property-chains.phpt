--TEST--
Paths with several property steps ($this->a->b[$k], $x->a[$k]->b), run often enough for the loop to take them, next to what must still go the long way: missing keys and properties, null on the way, another class at the same site, scope, magic, hooks, typed properties not initialized, ArrayAccess, references and a lazy ghost.
--FILE--
<?php
// Paths with several property steps ($this->a->b[$k], $x->a[$k]->b), run
// often enough for the loop to take them, next to everything that must still
// go the long way: a missing key or property, null on the way, another
// class at the same site, scope, magic, hooks, lazy objects, references,
// ArrayAccess and typed properties not yet initialized.

class Mapping { public $fieldMappings = ['id' => 'int', 'name' => 'string']; public ?Mapping $next = null; public $tag = 'm'; }
class OtherMapping { public $fieldMappings = ['id' => 'other']; public $tag = 'o'; }
class Field { public function __construct(public ?string $enumType = null) {} }
class Secret { private $hidden = ['k' => 1]; public $open = ['k' => 2]; }
class Magic { public function __get($name) { echo "__get($name) "; return $name === 'tag' ? 'magic' : ['k' => 'magic']; } public function __isset($name) { echo "__isset($name) "; return true; } }
class Hooked { public array $list { get => ['k' => 'hooked']; } }
class Typed { public array $list; }
class Bag implements ArrayAccess {
    function offsetExists(mixed $o): bool { echo "exists($o) "; return true; }
    function offsetGet(mixed $o): mixed { echo "get($o) "; return 'bag'; }
    function offsetSet(mixed $o, mixed $v): void {}
    function offsetUnset(mixed $o): void {}
}

class Hydrator {
    public $rsm;
    public function __construct($rsm) { $this->rsm = $rsm; }
    public function column($key) {
        return [
            isset($this->rsm->fieldMappings[$key]),
            $this->rsm->fieldMappings[$key] ?? 'none',
            empty($this->rsm->fieldMappings[$key]),
            strtoupper($this->rsm->tag),
        ];
    }
    public function read($key) { return $this->rsm->fieldMappings[$key]; }
    public function deep() { return $this->rsm->next->fieldMappings['id'] ?? 'no next'; }
    public function plain() { return $this->rsm->tag; }
}

function enum_type($class, $name) { return empty($class->fieldMappings[$name]->enumType) ? 'plain' : $class->fieldMappings[$name]->enumType; }

$h = new Hydrator(new Mapping);
for ($i = 0; $i < 3; $i++) {
    echo json_encode($h->column('id')), json_encode($h->column('missing')), ' ', $h->read('name'), ' ', $h->deep(), ' ', $h->plain(), "\n";
}
echo $h->read('missing'), "|\n";
$h->rsm->next = new Mapping;
$h->rsm->next->fieldMappings['id'] = 'deep';
echo $h->deep(), "\n";
// Another class at the same site.
$h->rsm = new OtherMapping;
for ($i = 0; $i < 2; $i++) { echo json_encode($h->column('id')), ' ', $h->plain(), ' ', $h->deep(), "\n"; }
// Null on the way.
$h->rsm = null;
echo json_encode($h->column('id')), "\n";
echo $h->read('id'), "|\n";
try { echo $h->plain(), "|\n"; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
// A scalar or an array where an object is expected.
$h->rsm = 5;
echo json_encode($h->column('id')), "\n";
$h->rsm = ['fieldMappings' => ['id' => 'array']];
echo json_encode($h->column('id')), "\n";
// Scope: a private property seen from outside.
class Reader { public $s; function get() { return $this->s->hidden['k'] ?? 'unseen'; } function open() { return $this->s->open['k']; } }
$r = new Reader; $r->s = new Secret;
for ($i = 0; $i < 2; $i++) { echo $r->open(), ' '; try { echo $r->get(); } catch (Error $e) { echo $e->getMessage(); } echo "\n"; }
// Magic, hooks, typed and not initialized, ArrayAccess.
$h->rsm = new Magic;
echo json_encode($h->column('k')), "\n";
class HookReader { public $o; function get() { return $this->o->list['k'] ?? 'none'; } }
$hr = new HookReader; $hr->o = new Hooked;
for ($i = 0; $i < 2; $i++) echo $hr->get(), ' ';
$hr->o = new Typed;
echo $hr->get(), ' ';
try { echo (function () { return $this->o->list['k']; })->call($hr); } catch (Error $e) { echo $e->getMessage(); }
echo "\n";
class BagHolder { public $bag; function get() { return $this->bag->items['k'] ?? 'none'; } }
$bh = new BagHolder; $bh->bag = new class { public $items; };
$bh->bag->items = new Bag;
for ($i = 0; $i < 2; $i++) echo $bh->get(), ' ';
echo "\n";
// A reference in the array, and a property unset on the way.
$m = new Mapping;
$x = 'before';
$m->fieldMappings['ref'] = &$x;
$h->rsm = $m;
for ($i = 0; $i < 2; $i++) { echo $h->read('ref'), ' '; $x = 'after'; }
unset($m->fieldMappings);
echo json_encode($h->column('id')), "\n";
echo $h->read('id'), "|\n";
// A lazy ghost in the middle.
$reflector = new ReflectionClass(Mapping::class);
$ghost = $reflector->newLazyGhost(function ($object) { echo "init "; $object->fieldMappings = ['id' => 'ghost']; });
$h->rsm = new Mapping;
for ($i = 0; $i < 2; $i++) $h->read('id');
$h->rsm = $ghost;
echo $h->read('id'), ' ', $h->read('id'), "\n";
// A property after a key, a function's local, with another class later.
$class = new class { public $fieldMappings; };
$class->fieldMappings = ['a' => new Field('Suit'), 'b' => new Field(), 'c' => null];
for ($i = 0; $i < 3; $i++) echo enum_type($class, 'a'), enum_type($class, 'b'), enum_type($class, 'zz'), enum_type($class, 'c'), ' ';
$class->fieldMappings['a'] = new class { public $enumType = 'anon'; };
echo enum_type($class, 'a'), "\n";
echo (function ($c) { return $c->fieldMappings['b']->enumType; })($class) ?? 'null', "\n";
echo (function ($c) { return $c->fieldMappings['zz']->enumType; })($class), "|\n";

--EXPECTF--
[true,"int",false,"M"][false,"none",true,"M"] string no next m
[true,"int",false,"M"][false,"none",true,"M"] string no next m
[true,"int",false,"M"][false,"none",true,"M"] string no next m

Warning: Undefined array key "missing" in %s on line 33
|
deep
[true,"other",false,"O"] o no next
[true,"other",false,"O"] o no next

Warning: Attempt to read property "tag" on null in %s on line 30

Deprecated: strtoupper(): Passing null to parameter #1 ($string) of type string is deprecated in %s on line 30
[false,"none",true,""]

Warning: Attempt to read property "fieldMappings" on null in %s on line 33

Warning: Trying to access array offset on null in %s on line 33
|

Warning: Attempt to read property "tag" on null in %s on line 35
|

Warning: Attempt to read property "tag" on int in %s on line 30

Deprecated: strtoupper(): Passing null to parameter #1 ($string) of type string is deprecated in %s on line 30
[false,"none",true,""]

Warning: Attempt to read property "tag" on array in %s on line 30

Deprecated: strtoupper(): Passing null to parameter #1 ($string) of type string is deprecated in %s on line 30
[false,"none",true,""]
2 unseen
2 unseen
__isset(fieldMappings) __get(fieldMappings) __isset(fieldMappings) __get(fieldMappings) __isset(fieldMappings) __get(fieldMappings) __get(tag) [true,"magic",false,"MAGIC"]
hooked hooked none Typed property Typed::$list must not be accessed before initialization
exists(k) get(k) bag exists(k) get(k) bag 
before after [false,"none",true,"M"]

Warning: Undefined property: Mapping::$fieldMappings in %s on line 33

Warning: Trying to access array offset on null in %s on line 33
|
init ghost ghost
Suitplainplainplain Suitplainplainplain Suitplainplainplain anon
null

Warning: Undefined array key "zz" in %s on line 103

Warning: Attempt to read property "enumType" on null in %s on line 103
|
