--TEST--
Undeclared properties written in the loop: stdClass, AllowDynamicProperties, the creation deprecation, __set, a readonly class, references, order, a lazy ghost, json_decode rows
--FILE--
<?php
#[AllowDynamicProperties] class Allowed { public int $n = 0; }
class Warns { public int $n = 0; }
class Magic { public function __set($name, $value) { echo "  __set($name)\n"; } }
class MagicExisting { public function __set($name, $value) { echo "  __set($name)\n"; } }
readonly class Frozen { public function __construct(public int $x = 1) {} }
function fill(object $o, int $times): void { for ($i = 0; $i < $times; $i++) { $o->dyn = $i; $o->{"k$i"} = $i; } }
foreach ([new stdClass, new Allowed, new Warns] as $o) {
    fill($o, 3);
    echo get_class($o), ': ', json_encode(get_object_vars($o)), "\n";
}
$m = new Magic; fill($m, 2); echo json_encode(get_object_vars($m)), "\n";
try { $f = new Frozen; $f->dyn = 1; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
// A reference in an undeclared property is written through.
$s = new stdClass; $x = 1; $s->r = &$x; for ($i = 0; $i < 3; $i++) { $s->r = $i * 10; } echo "x=$x\n";
// Unset and set again: the property goes to the end, as PHP keeps it.
$t = new stdClass; $t->a = 1; $t->b = 2; unset($t->a); $t->a = 3; echo implode(',', array_keys((array) $t)), "\n";
// A lazy ghost of a class that allows dynamic properties.
$ghost = (new ReflectionClass(Allowed::class))->newLazyGhost(function ($o) { echo "  init\n"; $o->n = 5; });
$ghost->extra = 'e'; echo $ghost->n, ' ', $ghost->extra, "\n";
// json_decode objects and rows written in a loop.
$rows = json_decode('[{"id":1,"name":"a"},{"id":2,"name":"b"}]');
foreach ($rows as $row) { $row->seen = true; $row->id += 100; }
echo json_encode($rows), "\n";

--EXPECTF--
stdClass: {"dyn":2,"k0":0,"k1":1,"k2":2}
Allowed: {"n":0,"dyn":2,"k0":0,"k1":1,"k2":2}

Deprecated: Creation of dynamic property Warns::$dyn is deprecated in %s on line 7

Deprecated: Creation of dynamic property Warns::$k0 is deprecated in %s on line 7

Deprecated: Creation of dynamic property Warns::$k1 is deprecated in %s on line 7

Deprecated: Creation of dynamic property Warns::$k2 is deprecated in %s on line 7
Warns: {"n":0,"dyn":2,"k0":0,"k1":1,"k2":2}
  __set(dyn)
  __set(k0)
  __set(dyn)
  __set(k1)
[]
Error: Cannot create dynamic property Frozen::$dyn
x=20
b,a
  init
5 e
[{"id":101,"name":"a","seen":true},{"id":102,"name":"b","seen":true}]
