--TEST--
ReflectionProperty reads and writes called from method sites: typed properties and their coercion, readonly, hooks, asymmetric visibility, a private property of a parent class, a subclass that redeclares a property, objects of the wrong class, uninitialized and unset properties, magic, references, lazy objects
--FILE--
<?php
set_error_handler(function ($no, $msg) { echo "  [$msg]\n"; return true; });
// ReflectionProperty reads and writes called from method sites in functions, methods and loops.
class Base { private int $secret = 1; protected ?string $label = null; public $plain = 'p'; }
class Product extends Base {
    public int $id;
    public string $name = 'n';
    public float $price = 0.0;
    public ?DateTimeImmutable $created = null;
    public readonly int $fixed;
    public array $tags = [];
    public static int $count = 0;
    public int $hooked { get => $this->hooked * 2; set => $this->hooked = $value + 1; }
    public private(set) int $asym = 4;
}
class Redeclared extends Product { public string $name = 'redeclared'; public int $extra = 0; }
class Other { public int $id = 5; }
class Noisy { public $v = 'noisy'; function __destruct() { echo "destruct ", $this->v, "\n"; } }
class Magic { public int $typed; function __get($n) { echo "__get $n\n"; return 'g'; } function __set($n, $v) { echo "__set $n\n"; } function __isset($n) { echo "__isset $n\n"; return true; } }
class MyProp extends ReflectionProperty { function getValue(?object $o = null): mixed { return 'overridden'; } }

function show(string $label, callable $f): void {
    try { $r = $f(); echo "$label: ", var_export($r, true), "\n"; }
    catch (Throwable $e) { echo "$label: ", get_class($e), ': ', $e->getMessage(), "\n"; }
}
function get(ReflectionProperty $rp, $o) { return $rp->getValue($o); }
function set(ReflectionProperty $rp, $o, $v) { return $rp->setValue($o, $v); }
function raw(ReflectionProperty $rp, $o, $v) { return $rp->setRawValueWithoutLazyInitialization($o, $v); }
function init(ReflectionProperty $rp, $o) { return $rp->isInitialized($o); }

$id = new ReflectionProperty(Product::class, 'id');
$name = new ReflectionProperty(Product::class, 'name');
$price = new ReflectionProperty(Product::class, 'price');
$secret = new ReflectionProperty(Base::class, 'secret');
$label = new ReflectionProperty(Base::class, 'label');
$created = new ReflectionProperty(Product::class, 'created');
$fixed = new ReflectionProperty(Product::class, 'fixed');
$tags = new ReflectionProperty(Product::class, 'tags');
$hooked = new ReflectionProperty(Product::class, 'hooked');
$asym = new ReflectionProperty(Product::class, 'asym');
$redeclaredName = new ReflectionProperty(Redeclared::class, 'name');

$p = new Product;
$r = new Redeclared;
for ($i = 0; $i < 2; $i++) {
    echo "-- round $i\n";
    foreach ([['product', $p], ['redeclared', $r], ['other', new Other]] as [$what, $o]) {
        show("$what id init", fn () => init($id, $o));
        show("$what id get", fn () => get($id, $o));
        show("$what id set int", fn () => set($id, $o, 10 + $i));
        show("$what id set numeric string", fn () => set($id, $o, '7'));
        show("$what id set float", fn () => set($id, $o, 3.0));
        show("$what id set fraction", fn () => set($id, $o, 3.5));
        show("$what id set null", fn () => set($id, $o, null));
        show("$what id get", fn () => get($id, $o));
        show("$what name set int", fn () => set($name, $o, 42));
        show("$what name get", fn () => get($name, $o));
        show("$what price set int", fn () => set($price, $o, 3));
        show("$what price get", fn () => get($price, $o));
        show("$what secret get", fn () => get($secret, $o));
        show("$what secret raw", fn () => raw($secret, $o, 9));
        show("$what label set", fn () => set($label, $o, 'L'));
        show("$what label get", fn () => get($label, $o));
        show("$what tags set", fn () => set($tags, $o, ['a' => 1]));
        show("$what tags set string", fn () => set($tags, $o, 'x'));
        show("$what created set", fn () => set($created, $o, new DateTimeImmutable('2020-01-02')));
        show("$what created set wrong", fn () => set($created, $o, new stdClass));
        show("$what fixed set", fn () => set($fixed, $o, 3));
        show("$what fixed get", fn () => get($fixed, $o));
        show("$what hooked set", fn () => set($hooked, $o, 5));
        show("$what hooked get", fn () => get($hooked, $o));
        show("$what hooked raw", fn () => raw($hooked, $o, 5));
        show("$what asym set", fn () => set($asym, $o, 8));
        show("$what asym get", fn () => get($asym, $o));
        show("$what redeclared name set", fn () => set($redeclaredName, $o, 'rn'));
        show("$what redeclared name get", fn () => get($redeclaredName, $o));
    }
}
echo "-- the same variable twice, a reference, a literal, a temporary receiver\n";
show('same variable', fn () => set($tags, $p, $p));
show('tags after', fn () => $tags->getValue($p) === $p);
$p->tags = [];
$value = ['by', 'reference'];
$ref = &$value;
show('reference argument', fn () => $tags->setValue($p, $value));
show('reference argument get', fn () => $tags->getValue($p));
show('literal', fn () => $name->setValue($p, 'lit'));
show('temporary receiver', fn () => (new ReflectionProperty(Product::class, 'name'))->getValue($p));
show('result into the argument', function () use ($name, $p) { $v = $p; $v = $name->getValue($p); return $v; });
show('no arguments', fn () => $name->getValue());
show('three arguments', fn () => $name->getValue($p, 1, 2));
show('wrong argument type', fn () => $name->getValue('x'));
show('null argument', fn () => $name->getValue(null));
show('one argument to setValue', fn () => $name->setValue($p));
show('named argument', fn () => $name->setValue(objectOrValue: $p, value: 'named'));
show('named get', fn () => $name->getValue(object: $p));
show('spread', fn () => $name->setValue(...[$p, 'spread']));
show('name now', fn () => $name->getValue($p));
show('overridden getValue', fn () => (new MyProp(Product::class, 'name'))->getValue($p));
echo "-- uninitialized, unset, magic\n";
$u = new Product;
show('uninitialized get', fn () => get($id, $u));
show('uninitialized init', fn () => init($id, $u));
unset($u->name);
show('unset init', fn () => init($name, $u));
show('unset get', fn () => get($name, $u));
show('unset set', fn () => set($name, $u, 'back'));
show('unset set order', fn () => implode(',', array_keys(get_object_vars($u))));
$m = new Magic;
$mt = new ReflectionProperty(Magic::class, 'typed');
show('magic uninit init', fn () => init($mt, $m));
show('magic uninit get', fn () => get($mt, $m));
show('magic uninit set', fn () => set($mt, $m, 3));
show('magic set get', fn () => get($mt, $m));
echo "-- destructors\n";
$n = new Product;
$tagsProp = new ReflectionProperty(Product::class, 'tags');
$nd = new ReflectionProperty(Redeclared::class, 'extra');
$holder = new class { public $v; };
$vp = new ReflectionProperty($holder, 'v');
$vp->setValue($holder, new Noisy);
echo "replacing\n";
$vp->setValue($holder, 'plain');
echo "replaced\n";
$vp->setValue($holder, new Noisy);
$vp->setRawValueWithoutLazyInitialization($holder, null);
echo "raw replaced\n";
echo "-- lazy\n";
$rc = new ReflectionClass(Product::class);
$ghost = $rc->newLazyGhost(function ($o) { echo "init ghost\n"; $o->id = 99; });
show('ghost get id', fn () => get($id, $ghost));
$ghost = $rc->newLazyGhost(function ($o) { echo "init ghost\n"; $o->id = 99; });
show('ghost raw set', fn () => raw($name, $ghost, 'g'));
show('ghost still lazy', fn () => $rc->isUninitializedLazyObject($ghost));
show('ghost set', fn () => set($name, $ghost, 'h'));
show('ghost lazy after', fn () => $rc->isUninitializedLazyObject($ghost));
show('ghost id', fn () => get($id, $ghost));
$proxy = $rc->newLazyProxy(function ($o) { echo "init proxy\n"; $q = new Product; $q->name = 'real'; return $q; });
show('proxy raw set', fn () => raw($price, $proxy, 1.5));
show('proxy get', fn () => get($name, $proxy));
echo "-- dynamic and static\n";
$std = new stdClass; $std->dyn = 1;
$dyn = new ReflectionProperty($std, 'dyn');
show('dynamic get', fn () => get($dyn, $std));
show('dynamic set', fn () => set($dyn, $std, 2));
show('dynamic get', fn () => get($dyn, $std));
$count = new ReflectionProperty(Product::class, 'count');
show('static get', fn () => $count->getValue());
show('static set', fn () => $count->setValue(5));
show('static get', fn () => $count->getValue());
show('static raw', fn () => $count->setRawValueWithoutLazyInitialization($p, 1));

--EXPECT--
-- round 0
product id init: false
product id get: Error: Typed property Product::$id must not be accessed before initialization
product id set int: NULL
product id set numeric string: NULL
product id set float: NULL
  [Implicit conversion from float 3.5 to int loses precision]
product id set fraction: NULL
product id set null: TypeError: Cannot assign null to property Product::$id of type int
product id get: 3
product name set int: NULL
product name get: '42'
product price set int: NULL
product price get: 3.0
product secret get: 1
product secret raw: NULL
product label set: NULL
product label get: 'L'
product tags set: NULL
product tags set string: TypeError: Cannot assign string to property Product::$tags of type array
product created set: NULL
product created set wrong: TypeError: Cannot assign stdClass to property Product::$created of type ?DateTimeImmutable
product fixed set: NULL
product fixed get: 3
product hooked set: NULL
product hooked get: 12
product hooked raw: NULL
product asym set: NULL
product asym get: 8
product redeclared name set: NULL
product redeclared name get: ReflectionException: Given object is not an instance of the class this property was declared in
redeclared id init: false
redeclared id get: Error: Typed property Product::$id must not be accessed before initialization
redeclared id set int: NULL
redeclared id set numeric string: NULL
redeclared id set float: NULL
  [Implicit conversion from float 3.5 to int loses precision]
redeclared id set fraction: NULL
redeclared id set null: TypeError: Cannot assign null to property Product::$id of type int
redeclared id get: 3
redeclared name set int: NULL
redeclared name get: '42'
redeclared price set int: NULL
redeclared price get: 3.0
redeclared secret get: 1
redeclared secret raw: NULL
redeclared label set: NULL
redeclared label get: 'L'
redeclared tags set: NULL
redeclared tags set string: TypeError: Cannot assign string to property Product::$tags of type array
redeclared created set: NULL
redeclared created set wrong: TypeError: Cannot assign stdClass to property Product::$created of type ?DateTimeImmutable
redeclared fixed set: NULL
redeclared fixed get: 3
redeclared hooked set: NULL
redeclared hooked get: 12
redeclared hooked raw: NULL
redeclared asym set: NULL
redeclared asym get: 8
redeclared redeclared name set: NULL
redeclared redeclared name get: 'rn'
other id init: ReflectionException: Given object is not an instance of the class this property was declared in
other id get: ReflectionException: Given object is not an instance of the class this property was declared in
other id set int: NULL
other id set numeric string: NULL
other id set float: NULL
  [Implicit conversion from float 3.5 to int loses precision]
other id set fraction: NULL
other id set null: TypeError: Cannot assign null to property Other::$id of type int
other id get: ReflectionException: Given object is not an instance of the class this property was declared in
  [Creation of dynamic property Other::$name is deprecated]
other name set int: NULL
other name get: ReflectionException: Given object is not an instance of the class this property was declared in
  [Creation of dynamic property Other::$price is deprecated]
other price set int: NULL
other price get: ReflectionException: Given object is not an instance of the class this property was declared in
other secret get: ReflectionException: Given object is not an instance of the class this property was declared in
other secret raw: TypeError: ReflectionProperty::setRawValueWithoutLazyInitialization(): Argument #1 ($object) must be of type Base, Other given
  [Creation of dynamic property Other::$label is deprecated]
other label set: NULL
other label get: ReflectionException: Given object is not an instance of the class this property was declared in
  [Creation of dynamic property Other::$tags is deprecated]
other tags set: NULL
other tags set string: NULL
  [Creation of dynamic property Other::$created is deprecated]
other created set: NULL
other created set wrong: NULL
  [Creation of dynamic property Other::$fixed is deprecated]
other fixed set: NULL
other fixed get: ReflectionException: Given object is not an instance of the class this property was declared in
  [Creation of dynamic property Other::$hooked is deprecated]
other hooked set: NULL
other hooked get: ReflectionException: Given object is not an instance of the class this property was declared in
other hooked raw: TypeError: ReflectionProperty::setRawValueWithoutLazyInitialization(): Argument #1 ($object) must be of type Product, Other given
  [Creation of dynamic property Other::$asym is deprecated]
other asym set: NULL
other asym get: ReflectionException: Given object is not an instance of the class this property was declared in
other redeclared name set: NULL
other redeclared name get: ReflectionException: Given object is not an instance of the class this property was declared in
-- round 1
product id init: true
product id get: 3
product id set int: NULL
product id set numeric string: NULL
product id set float: NULL
  [Implicit conversion from float 3.5 to int loses precision]
product id set fraction: NULL
product id set null: TypeError: Cannot assign null to property Product::$id of type int
product id get: 3
product name set int: NULL
product name get: '42'
product price set int: NULL
product price get: 3.0
product secret get: 9
product secret raw: NULL
product label set: NULL
product label get: 'L'
product tags set: NULL
product tags set string: TypeError: Cannot assign string to property Product::$tags of type array
product created set: NULL
product created set wrong: TypeError: Cannot assign stdClass to property Product::$created of type ?DateTimeImmutable
product fixed set: Error: Cannot modify readonly property Product::$fixed
product fixed get: 3
product hooked set: NULL
product hooked get: 12
product hooked raw: NULL
product asym set: NULL
product asym get: 8
product redeclared name set: NULL
product redeclared name get: ReflectionException: Given object is not an instance of the class this property was declared in
redeclared id init: true
redeclared id get: 3
redeclared id set int: NULL
redeclared id set numeric string: NULL
redeclared id set float: NULL
  [Implicit conversion from float 3.5 to int loses precision]
redeclared id set fraction: NULL
redeclared id set null: TypeError: Cannot assign null to property Product::$id of type int
redeclared id get: 3
redeclared name set int: NULL
redeclared name get: '42'
redeclared price set int: NULL
redeclared price get: 3.0
redeclared secret get: 9
redeclared secret raw: NULL
redeclared label set: NULL
redeclared label get: 'L'
redeclared tags set: NULL
redeclared tags set string: TypeError: Cannot assign string to property Product::$tags of type array
redeclared created set: NULL
redeclared created set wrong: TypeError: Cannot assign stdClass to property Product::$created of type ?DateTimeImmutable
redeclared fixed set: Error: Cannot modify readonly property Product::$fixed
redeclared fixed get: 3
redeclared hooked set: NULL
redeclared hooked get: 12
redeclared hooked raw: NULL
redeclared asym set: NULL
redeclared asym get: 8
redeclared redeclared name set: NULL
redeclared redeclared name get: 'rn'
other id init: ReflectionException: Given object is not an instance of the class this property was declared in
other id get: ReflectionException: Given object is not an instance of the class this property was declared in
other id set int: NULL
other id set numeric string: NULL
other id set float: NULL
  [Implicit conversion from float 3.5 to int loses precision]
other id set fraction: NULL
other id set null: TypeError: Cannot assign null to property Other::$id of type int
other id get: ReflectionException: Given object is not an instance of the class this property was declared in
  [Creation of dynamic property Other::$name is deprecated]
other name set int: NULL
other name get: ReflectionException: Given object is not an instance of the class this property was declared in
  [Creation of dynamic property Other::$price is deprecated]
other price set int: NULL
other price get: ReflectionException: Given object is not an instance of the class this property was declared in
other secret get: ReflectionException: Given object is not an instance of the class this property was declared in
other secret raw: TypeError: ReflectionProperty::setRawValueWithoutLazyInitialization(): Argument #1 ($object) must be of type Base, Other given
  [Creation of dynamic property Other::$label is deprecated]
other label set: NULL
other label get: ReflectionException: Given object is not an instance of the class this property was declared in
  [Creation of dynamic property Other::$tags is deprecated]
other tags set: NULL
other tags set string: NULL
  [Creation of dynamic property Other::$created is deprecated]
other created set: NULL
other created set wrong: NULL
  [Creation of dynamic property Other::$fixed is deprecated]
other fixed set: NULL
other fixed get: ReflectionException: Given object is not an instance of the class this property was declared in
  [Creation of dynamic property Other::$hooked is deprecated]
other hooked set: NULL
other hooked get: ReflectionException: Given object is not an instance of the class this property was declared in
other hooked raw: TypeError: ReflectionProperty::setRawValueWithoutLazyInitialization(): Argument #1 ($object) must be of type Product, Other given
  [Creation of dynamic property Other::$asym is deprecated]
other asym set: NULL
other asym get: ReflectionException: Given object is not an instance of the class this property was declared in
other redeclared name set: NULL
other redeclared name get: ReflectionException: Given object is not an instance of the class this property was declared in
-- the same variable twice, a reference, a literal, a temporary receiver
same variable: TypeError: Cannot assign Product to property Product::$tags of type array
tags after: false
reference argument: NULL
reference argument get: array (
  0 => 'by',
  1 => 'reference',
)
literal: NULL
temporary receiver: 'lit'
result into the argument: 'lit'
no arguments: TypeError: ReflectionProperty::getValue(): Argument #1 ($object) must be provided for instance properties
three arguments: ArgumentCountError: ReflectionProperty::getValue() expects at most 1 argument, 3 given
wrong argument type: TypeError: ReflectionProperty::getValue(): Argument #1 ($object) must be of type ?object, string given
null argument: TypeError: ReflectionProperty::getValue(): Argument #1 ($object) must be provided for instance properties
one argument to setValue: ArgumentCountError: ReflectionProperty::setValue() expects exactly 2 arguments, 1 given
named argument: NULL
named get: 'named'
spread: NULL
name now: 'spread'
overridden getValue: 'overridden'
-- uninitialized, unset, magic
uninitialized get: Error: Typed property Product::$id must not be accessed before initialization
uninitialized init: false
unset init: false
unset get: Error: Typed property Product::$name must not be accessed before initialization
unset set: NULL
unset set order: Error: Typed property Product::$hooked must not be accessed before initialization
magic uninit init: false
magic uninit get: Error: Typed property Magic::$typed must not be accessed before initialization
magic uninit set: NULL
magic set get: 3
-- destructors
replacing
destruct noisy
replaced
destruct noisy
raw replaced
-- lazy
init ghost
ghost get id: 99
ghost raw set: NULL
ghost still lazy: true
ghost set: NULL
ghost lazy after: true
init ghost
ghost id: 99
proxy raw set: NULL
init proxy
proxy get: 'real'
-- dynamic and static
dynamic get: 1
dynamic set: NULL
dynamic get: 2
static get: 0
  [Calling ReflectionProperty::setValue() with a single argument is deprecated]
static set: NULL
static get: 5
static raw: ReflectionException: Can not use setRawValueWithoutLazyInitialization on static property Product::$count
