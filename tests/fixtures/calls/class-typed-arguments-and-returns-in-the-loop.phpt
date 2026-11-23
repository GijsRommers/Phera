--TEST--
Class-typed parameters and returns of subclasses, interfaces, unions, nullable types, self and static; TypeErrors through the VM
--FILE--
<?php
namespace App;
abstract class Type { public function name(): string { return static::class; } }
final class StringType extends Type {} final class IntType extends Type {} final class JsonType extends Type {}
interface Shape {} final class Square implements Shape {} final class Circle implements Shape {}
class Base { public function me(): static { return $this; } public function same(): self { return $this; } }
final class Child extends Base {}
final class Registry {
    private array $types;
    public function __construct() { $this->types = ['s' => new StringType, 'i' => new IntType, 'j' => new JsonType]; }
    public function get(string $name): Type { return $this->types[$name]; }
    public function maybe(?string $name): ?Type { return $name === null ? null : $this->types[$name]; }
    public function either(string $name): Type|Shape { return $name === 'sq' ? new Square : $this->types[$name]; }
}
function area(Shape $shape): string { return $shape::class; }
function takeType(Type $t, ?Shape $s = null): string { return $t->name() . ($s ? '+' . $s::class : ''); }
$r = new Registry; $out = [];
for ($i = 0; $i < 3; $i++) {
    foreach (['s', 'i', 'j', 's', 'j'] as $n) { $out[] = $r->get($n)->name(); }
    $out[] = $r->maybe(null) === null ? 'null' : 'x';
    $out[] = $r->either('sq')::class; $out[] = $r->either('i')::class;
    $out[] = area(new Square); $out[] = area(new Circle);
    $out[] = takeType(new IntType, new Circle); $out[] = takeType(new JsonType);
    $out[] = (new Child)->me()::class; $out[] = (new Child)->same()::class;
}
echo implode("\n", array_unique($out)), "\n";
function wrongReturn(): Type { return new Square; }
function wrongParam(Type $t) { return 1; }
function undeclared(): \Missing\Thing { return new Square; }
foreach (['wrongReturn' => fn () => wrongReturn(), 'wrongParam' => fn () => wrongParam(new Circle), 'undeclared' => fn () => undeclared()] as $label => $f) {
    try { $f(); echo "$label: no error\n"; } catch (\TypeError $e) { echo "$label: ", $e->getMessage(), "\n"; }
}

--EXPECTF--
App\StringType
App\IntType
App\JsonType
null
App\Square
App\Circle
App\IntType+App\Circle
App\Child
wrongReturn: App\wrongReturn(): Return value must be of type App\Type, App\Square returned
wrongParam: App\wrongParam(): Argument #1 ($t) must be of type App\Type, App\Circle given, called in %s on line 30
undeclared: App\undeclared(): Return value must be of type Missing\Thing, App\Square returned
