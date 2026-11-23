--TEST--
(array) of an object: mangled keys of private and protected properties, a private property of a parent with the same name, uninitialized and unset properties, hooks, readonly, dynamic and integer-like names, references, closures, native classes, lazy objects, and `((array) $o)[$key] ?? null`
--FILE--
<?php
set_error_handler(function ($no, $msg) { echo "  [$msg]\n"; return true; });
class Base { private int $secret = 1; protected ?string $label = 'b'; public $plain = 'p'; private $shadow = 'base'; }
class Product extends Base {
    public int $id;
    public string $name = 'n';
    public float $price = 1.5;
    public ?Product $parent = null;
    public readonly int $fixed;
    public array $tags = ['a', 'b'];
    private $shadow = 'product';
    public int $hooked { get => $this->hooked * 2; set => $this->hooked = $value + 1; }
    public static int $count = 7;
    function __construct() { $this->fixed = 4; }
}
class Redeclared extends Product { public string $name = 'redeclared'; public int $extra = 0; }
#[AllowDynamicProperties] class Dyn { public $a = 1; }
class Plain { public $x = 1; protected $y = 2; private $z = 3; }
class WithDestructor { public $v = 'v'; function __destruct() { echo "destruct\n"; } }

function dump(string $label, array $a): void {
    echo $label, ': ', count($a), "\n";
    foreach ($a as $k => $v) {
        echo '  ', is_int($k) ? "int $k" : str_replace("\0", '\0', $k), ' => ';
        echo is_object($v) ? get_class($v) : (is_array($v) ? 'array(' . count($v) . ')' : var_export($v, true)), "\n";
    }
}
for ($i = 0; $i < 2; $i++) {
    echo "-- round $i\n";
    $p = new Product;
    dump('uninitialized id and hooked', (array) $p);
    $p->id = 3; $p->hooked = 5;
    dump('initialized', (array) $p);
    $r = new Redeclared;
    dump('redeclared', (array) $r);
    unset($p->name);
    dump('name unset', (array) $p);
    $p->name = 'again';
    dump('name set again', (array) $p);
    $p->dynamic = 'd';
    dump('dynamic property', (array) $p);
    $plain = new Plain;
    dump('plain', (array) $plain);
    echo 'read private: ', var_export(((array) $plain)["\0Plain\0z"] ?? null, true), "\n";
    echo 'read protected: ', var_export(((array) $plain)["\0*\0y"] ?? null, true), "\n";
    echo 'read public: ', var_export(((array) $plain)['x'] ?? null, true), "\n";
    echo 'read missing: ', var_export(((array) $plain)["\0Other\0z"] ?? null, true), "\n";
    echo 'read shadowed parent: ', var_export(((array) $p)["\0Base\0shadow"] ?? null, true), "\n";
    echo 'read shadowed own: ', var_export(((array) $p)["\0Product\0shadow"] ?? null, true), "\n";
}
echo "-- references\n";
$o = new Plain;
$ref = &$o->x;
$a = (array) $o;
$a['x'] = 99;
echo 'x is ', $o->x, " (a reference with two owners is shared)\n";
unset($ref);
$o2 = new Plain;
$r2 = &$o2->x; unset($r2);
$b = (array) $o2;
$b['x'] = 5;
echo 'x is ', $o2->x, " (a lone reference is a copy)\n";
$copy = (array) $o2;
$o2->x = 'changed';
echo 'copy keeps ', $copy['x'], "\n";
echo "-- stdClass, integer-like names\n";
$s = (object) ['a' => 1, 5 => 'five', '7' => 'seven'];
dump('stdClass', (array) $s);
$s->{'9'} = 'nine';
dump('stdClass added', (array) $s);
$d = new Dyn; $d->b = 2; $d->{'10'} = 'ten';
dump('dynamic class', (array) $d);
echo "-- other kinds\n";
dump('closure', (array) function () {});
dump('ArrayObject', (array) new ArrayObject([1, 2]));
dump('ArrayIterator', (array) new ArrayIterator(['k' => 'v']));
dump('DateTime', array_keys((array) new DateTime('2020-01-02 03:04:05 UTC')));
dump('Exception', array_map(fn ($k) => str_replace("\0", '\0', $k), array_keys((array) new Exception('m'))));
enum Suit: string { case Hearts = 'H'; }
dump('enum', (array) Suit::Hearts);
dump('scalar', (array) 5);
dump('null', (array) null);
dump('string', (array) 'str');
echo "-- lazy\n";
$rc = new ReflectionClass(Product::class);
$ghost = $rc->newLazyGhost(function ($o) { echo "init ghost\n"; $o->id = 99; });
dump('ghost cast', (array) $ghost);
echo "-- order and counts\n";
$n = new Plain;
$n->extra = 1;
unset($n->x);
$n->x = 'back';
dump('x back at its place', (array) $n);
echo "-- the object is not changed by the cast\n";
$w = new WithDestructor;
$arr = (array) $w;
unset($w);
echo "unset done\n";
unset($arr);
echo "end\n";

--EXPECT--
-- round 0
uninitialized id and hooked: 10
  \0Base\0secret => 1
  \0*\0label => 'b'
  plain => 'p'
  \0Base\0shadow => 'base'
  name => 'n'
  price => 1.5
  parent => NULL
  fixed => 4
  tags => array(2)
  \0Product\0shadow => 'product'
initialized: 12
  \0Base\0secret => 1
  \0*\0label => 'b'
  plain => 'p'
  \0Base\0shadow => 'base'
  id => 3
  name => 'n'
  price => 1.5
  parent => NULL
  fixed => 4
  tags => array(2)
  \0Product\0shadow => 'product'
  hooked => 6
redeclared: 11
  \0Base\0secret => 1
  \0*\0label => 'b'
  plain => 'p'
  \0Base\0shadow => 'base'
  name => 'redeclared'
  price => 1.5
  parent => NULL
  fixed => 4
  tags => array(2)
  \0Product\0shadow => 'product'
  extra => 0
name unset: 11
  \0Base\0secret => 1
  \0*\0label => 'b'
  plain => 'p'
  \0Base\0shadow => 'base'
  id => 3
  price => 1.5
  parent => NULL
  fixed => 4
  tags => array(2)
  \0Product\0shadow => 'product'
  hooked => 6
name set again: 12
  \0Base\0secret => 1
  \0*\0label => 'b'
  plain => 'p'
  \0Base\0shadow => 'base'
  id => 3
  name => 'again'
  price => 1.5
  parent => NULL
  fixed => 4
  tags => array(2)
  \0Product\0shadow => 'product'
  hooked => 6
  [Creation of dynamic property Product::$dynamic is deprecated]
dynamic property: 13
  \0Base\0secret => 1
  \0*\0label => 'b'
  plain => 'p'
  \0Base\0shadow => 'base'
  id => 3
  name => 'again'
  price => 1.5
  parent => NULL
  fixed => 4
  tags => array(2)
  \0Product\0shadow => 'product'
  hooked => 6
  dynamic => 'd'
plain: 3
  x => 1
  \0*\0y => 2
  \0Plain\0z => 3
read private: 3
read protected: 2
read public: 1
read missing: NULL
read shadowed parent: 'base'
read shadowed own: 'product'
-- round 1
uninitialized id and hooked: 10
  \0Base\0secret => 1
  \0*\0label => 'b'
  plain => 'p'
  \0Base\0shadow => 'base'
  name => 'n'
  price => 1.5
  parent => NULL
  fixed => 4
  tags => array(2)
  \0Product\0shadow => 'product'
initialized: 12
  \0Base\0secret => 1
  \0*\0label => 'b'
  plain => 'p'
  \0Base\0shadow => 'base'
  id => 3
  name => 'n'
  price => 1.5
  parent => NULL
  fixed => 4
  tags => array(2)
  \0Product\0shadow => 'product'
  hooked => 6
redeclared: 11
  \0Base\0secret => 1
  \0*\0label => 'b'
  plain => 'p'
  \0Base\0shadow => 'base'
  name => 'redeclared'
  price => 1.5
  parent => NULL
  fixed => 4
  tags => array(2)
  \0Product\0shadow => 'product'
  extra => 0
name unset: 11
  \0Base\0secret => 1
  \0*\0label => 'b'
  plain => 'p'
  \0Base\0shadow => 'base'
  id => 3
  price => 1.5
  parent => NULL
  fixed => 4
  tags => array(2)
  \0Product\0shadow => 'product'
  hooked => 6
name set again: 12
  \0Base\0secret => 1
  \0*\0label => 'b'
  plain => 'p'
  \0Base\0shadow => 'base'
  id => 3
  name => 'again'
  price => 1.5
  parent => NULL
  fixed => 4
  tags => array(2)
  \0Product\0shadow => 'product'
  hooked => 6
  [Creation of dynamic property Product::$dynamic is deprecated]
dynamic property: 13
  \0Base\0secret => 1
  \0*\0label => 'b'
  plain => 'p'
  \0Base\0shadow => 'base'
  id => 3
  name => 'again'
  price => 1.5
  parent => NULL
  fixed => 4
  tags => array(2)
  \0Product\0shadow => 'product'
  hooked => 6
  dynamic => 'd'
plain: 3
  x => 1
  \0*\0y => 2
  \0Plain\0z => 3
read private: 3
read protected: 2
read public: 1
read missing: NULL
read shadowed parent: 'base'
read shadowed own: 'product'
-- references
x is 99 (a reference with two owners is shared)
x is 1 (a lone reference is a copy)
copy keeps 1
-- stdClass, integer-like names
stdClass: 3
  a => 1
  int 5 => 'five'
  int 7 => 'seven'
stdClass added: 4
  a => 1
  int 5 => 'five'
  int 7 => 'seven'
  int 9 => 'nine'
dynamic class: 3
  a => 1
  b => 2
  int 10 => 'ten'
-- other kinds
closure: 1
  int 0 => Closure
ArrayObject: 2
  int 0 => 1
  int 1 => 2
ArrayIterator: 1
  k => 'v'
DateTime: 3
  int 0 => 'date'
  int 1 => 'timezone_type'
  int 2 => 'timezone'
Exception: 7
  int 0 => '\\0*\\0message'
  int 1 => '\\0Exception\\0string'
  int 2 => '\\0*\\0code'
  int 3 => '\\0*\\0file'
  int 4 => '\\0*\\0line'
  int 5 => '\\0Exception\\0trace'
  int 6 => '\\0Exception\\0previous'
enum: 2
  name => 'Hearts'
  value => 'H'
scalar: 1
  int 0 => 5
null: 0
string: 1
  int 0 => 'str'
-- lazy
ghost cast: 0
-- order and counts
  [Creation of dynamic property Plain::$extra is deprecated]
x back at its place: 4
  x => 'back'
  \0*\0y => 2
  \0Plain\0z => 3
  extra => 1
-- the object is not changed by the cast
destruct
unset done
end
