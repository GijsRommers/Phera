--TEST--
Reflection uses magic string conversion, object dumps and concrete Closure invocation
--FILE--
<?php
class MagicDisplay {
    public int $number = 5;
    public function __toString() { return 'display'; }
    public function __debugInfo() { return ['shown' => $this->number]; }
}
$object = new MagicDisplay();
echo $object, "\n";
var_dump($object);
$class = new ReflectionClass($object);
var_dump($class);
echo $class->getMethod('__toString')->getReturnType(), "\n";
echo (new ReflectionProperty($object, 'number'))->getType(), "\n";
$closure = static function (int $number = 5): int { return $number + 1; };
$reflection = new ReflectionFunction($closure);
$method = new ReflectionMethod($closure, '__invoke');
var_dump($reflection->getClosure() === $closure, $method->getClosure($closure) === $closure);
var_dump($method->isInternal(), $method->isClosure(), $method->getExtensionName());
echo $method->getParameters()[0]->getName(), " ", $method->getReturnType(), "\n";
var_dump($method->invokeArgs($closure, ['number' => 8]));
--EXPECTF--
display
object(MagicDisplay)#%d (1) {
  ["shown"]=>
  int(5)
}
object(ReflectionClass)#%d (1) {
  ["name"]=>
  string(12) "MagicDisplay"
}
string
int
bool(true)
bool(true)
bool(true)
bool(false)
bool(false)
number int
int(9)
