--TEST--
ReflectionMethod invokes by-reference arguments and binds static method closures
--FILE--
<?php
class ParentClass {
    public function reference(&$parameter) { return ++$parameter; }
    public static function called() { return static::class; }
}
class Child extends ParentClass {}
$r = new ReflectionClass(Child::class);
$m = $r->getMethod('reference');
$object = new Child();
$value = 3;
echo $m->invokeArgs($object, [&$value]), ':', $value, "\n";
set_error_handler(function ($severity, $message) {
    echo $message, "\n";
    return true;
});
echo $m->invoke($object, 4), "\n";
restore_error_handler();
$m = $r->getMethod('called');
echo $m->invoke(null), ':', $m->getClosure()(), "\n";
--EXPECT--
4:4
ParentClass::reference(): Argument #1 ($parameter) must be passed by reference, value given
5
Child:ParentClass
