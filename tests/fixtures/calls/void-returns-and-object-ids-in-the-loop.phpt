--TEST--
void returns in the loop; spl_object_id of objects, wrong types, named arguments and lazy objects
--FILE--
<?php
class Acc {
    public array $log = [];
    public function add(string $s): void { $this->log[] = $s; return; }
    public function addImplicit(string $s): void { $this->log[] = $s; }
    public function chain(): static { return $this; }
}
function ids(object ...$objects): array { return array_map(fn ($o) => spl_object_id($o) === spl_object_id($o), $objects); }
$a = new Acc;
for ($i = 0; $i < 3; $i++) { $a->add("x$i"); $a->addImplicit("y$i"); var_dump($a->add('z')); }
echo implode(',', $a->log), "\n";
$o1 = new stdClass; $o2 = new stdClass;
var_dump(spl_object_id($o1) === spl_object_id($o1), spl_object_id($o1) !== spl_object_id($o2), is_int(spl_object_id($o2)));
echo json_encode(ids($o1, $o2, $a)), "\n";
$id = spl_object_id($o1); unset($o1); $o3 = new stdClass; var_dump(spl_object_id($o3) === $id);
try { spl_object_id('nope'); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { spl_object_id(); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
var_dump(spl_object_id(object: $o2) === spl_object_id($o2));
$lazy = (new ReflectionClass(Acc::class))->newLazyGhost(function ($o) { echo "initialized\n"; });
spl_object_id($lazy); echo "after id\n"; var_dump((new ReflectionClass(Acc::class))->isUninitializedLazyObject($lazy));

--EXPECTF--
NULL
NULL
NULL
x0,y0,z,x1,y1,z,x2,y2,z
bool(true)
bool(true)
bool(true)
[true,true,true]
bool(true)
spl_object_id(): Argument #1 ($object) must be of type object, string given
spl_object_id() expects exactly 1 argument, 0 given
bool(true)
after id
bool(true)
