--TEST--
Reflection classes enumerate and access declared members
--FILE--
<?php
namespace Example;
interface Contract {}
class ParentClass {
    private int $hidden = 1;
    protected string $parentProperty = 'base';
    public function inherited() {}
    private const SECRET = 1;
    protected const string BASE = 'base';
}
final class Service extends ParentClass implements Contract {
    public string $name;
    private static int $count = 1;
    private int $value = 2;
    public const int VERSION = 7;

    public function __construct(string $name, private int $option = 3) { $this->name = $name; }
    public function own() {}
}
$r = new \ReflectionClass(Service::class);
echo $r->getName(), "\n";
echo $r->getNamespaceName(), " ", $r->getShortName(), "\n";
echo $r->getParentClass()->name, "\n";
var_dump($r->getInterfaceNames(), $r->isInstantiable(), $r->isFinal(), $r->getModifiers());
foreach ($r->getProperties() as $p) {
    echo $p->name, " ", $p->getModifiers(), " ", (int) $p->isPromoted(), "\n";
}
foreach ($r->getMethods() as $method) { echo $method->name, "\n"; }
var_dump($r->getConstants());
$x = $r->newInstanceArgs(['option' => 8, 'name' => 'test']);
var_dump($x->name, $r->getProperty('option')->getValue($x));
$r->getProperty('value')->setValue($x, 42);
var_dump($r->getProperty('value')->getValue($x));
$r->setStaticPropertyValue('count', 13);
var_dump($r->getStaticPropertyValue('count'), $r->getStaticProperties());
--EXPECT--
Example\Service
Example Service
Example\ParentClass
array(1) {
  [0]=>
  string(16) "Example\Contract"
}
bool(true)
bool(true)
int(32)
name 1 0
count 20 0
value 4 0
option 4 1
parentProperty 2 0
__construct
own
inherited
array(2) {
  ["VERSION"]=>
  int(7)
  ["BASE"]=>
  string(4) "base"
}
string(4) "test"
int(8)
int(42)
int(13)
array(1) {
  ["count"]=>
  int(13)
}
