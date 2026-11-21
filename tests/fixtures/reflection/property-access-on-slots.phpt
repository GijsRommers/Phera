--TEST--
ReflectionProperty reads and writes of declared, inherited private, typed, readonly, static and dynamic properties, of objects of another class, through references and of lazy ghosts
--FILE--
<?php
class Base { private int $secret = 1; protected ?string $label = null; public $plain = 'p'; }
class Product extends Base {
    public int $id;
    public string $name = 'n';
    public ?DateTimeImmutable $created = null;
    public readonly int $fixed;
    public array $tags = [];
    public static int $count = 0;
    public function __construct() {}
}
class Other { public int $id = 5; }
function show(string $label, callable $f) { try { $r = $f(); echo "$label: ", var_export($r, true), "\n"; } catch (Throwable $e) { echo "$label: ", get_class($e), ': ', $e->getMessage(), "\n"; } }
$rc = new ReflectionClass(Product::class);
$p = $rc->newInstanceWithoutConstructor();
$id = new ReflectionProperty(Product::class, 'id');
$name = new ReflectionProperty(Product::class, 'name');
$secret = new ReflectionProperty(Base::class, 'secret');
$label = new ReflectionProperty(Base::class, 'label');
$created = new ReflectionProperty(Product::class, 'created');
$fixed = new ReflectionProperty(Product::class, 'fixed');
$tags = new ReflectionProperty(Product::class, 'tags');
$count = new ReflectionProperty(Product::class, 'count');
for ($i = 0; $i < 2; $i++) {
    show('id init', fn () => $id->isInitialized($p));
    show('id get', fn () => $id->getValue($p));
    show('id raw set', fn () => $id->setRawValueWithoutLazyInitialization($p, 7));
    show('id get', fn () => $id->getValue($p));
    show('id set string', fn () => $id->setValue($p, '8'));
    show('id get', fn () => $id->getValue($p));
    show('id set bad', fn () => $id->setValue($p, 'x'));
    show('name get', fn () => $name->getValue($p));
    show('name set', fn () => $name->setValue($p, 'Desk'));
    show('name raw', fn () => $name->getRawValue($p));
    show('secret get', fn () => $secret->getValue($p));
    show('secret set', fn () => $secret->setValue($p, 2));
    show('label init', fn () => $label->isInitialized($p));
    show('created set', fn () => $created->setValue($p, new DateTimeImmutable('2020-01-01')));
    show('created get', fn () => $created->getValue($p)->format('Y'));
    show('fixed set', fn () => $fixed->setValue($p, 3));
    show('fixed init', fn () => $fixed->isInitialized($p));
    show('tags set', fn () => $tags->setValue($p, ['a']));
    show('isLazy', fn () => $id->isLazy($p));
    show('uninit lazy', fn () => $rc->isUninitializedLazyObject($p));
    show('count static', fn () => $count->getValue());
    show('wrong object', fn () => $id->getValue(new Other));
    show('wrong object init', fn () => $id->isInitialized(new Other));
}
unset($p->name);
show('name unset init', fn () => $name->isInitialized($p));
show('name unset get', fn () => $name->getValue($p));
show('name reset', fn () => $name->setValue($p, 'again'));
echo implode(',', array_keys(get_object_vars($p))), "\n";
$ref = 'r'; $p->name = &$ref;
show('name set through reference', fn () => $name->setValue($p, 'via'));
echo "ref is $ref\n";
$ghost = $rc->newLazyGhost(function ($o) { echo "  init ghost\n"; $o->id = 99; });
show('ghost isLazy', fn () => $id->isLazy($ghost));
show('ghost uninit', fn () => $rc->isUninitializedLazyObject($ghost));
show('ghost raw set', fn () => $name->setRawValueWithoutLazyInitialization($ghost, 'g'));
show('ghost uninit', fn () => $rc->isUninitializedLazyObject($ghost));
show('ghost get', fn () => $id->getValue($ghost));
show('ghost uninit', fn () => $rc->isUninitializedLazyObject($ghost));
$o = new stdClass; $o->dyn = 1;
show('dynamic', fn () => (new ReflectionProperty($o, 'dyn'))->getValue($o));

--EXPECTF--
id init: false
id get: Error: Typed property Product::$id must not be accessed before initialization
id raw set: NULL
id get: 7
id set string: NULL
id get: 8
id set bad: TypeError: Cannot assign string to property Product::$id of type int
name get: 'n'
name set: NULL
name raw: 'Desk'
secret get: 1
secret set: NULL
label init: true
created set: NULL
created get: '2020'
fixed set: NULL
fixed init: true
tags set: NULL
isLazy: false
uninit lazy: false
count static: 0
wrong object: ReflectionException: Given object is not an instance of the class this property was declared in
wrong object init: ReflectionException: Given object is not an instance of the class this property was declared in
id init: true
id get: 8
id raw set: NULL
id get: 7
id set string: NULL
id get: 8
id set bad: TypeError: Cannot assign string to property Product::$id of type int
name get: 'Desk'
name set: NULL
name raw: 'Desk'
secret get: 2
secret set: NULL
label init: true
created set: NULL
created get: '2020'
fixed set: Error: Cannot modify readonly property Product::$fixed
fixed init: true
tags set: NULL
isLazy: false
uninit lazy: false
count static: 0
wrong object: ReflectionException: Given object is not an instance of the class this property was declared in
wrong object init: ReflectionException: Given object is not an instance of the class this property was declared in
name unset init: false
name unset get: Error: Typed property Product::$name must not be accessed before initialization
name reset: NULL
plain,id,name,created,fixed,tags
name set through reference: NULL
ref is via
ghost isLazy: true
ghost uninit: true
ghost raw set: NULL
ghost uninit: true
  init ghost
ghost get: 99
ghost uninit: false
dynamic: 1
