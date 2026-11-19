--TEST--
A return type that narrows a built-in method to a class not loaded yet loads that class before the two are compared
--FILE--
<?php
// A child's return type names a class that is not loaded yet when the child is declared.
spl_autoload_register(function ($class) { echo "autoload $class\n"; if ($class === 'MyZone') { eval('class MyZone extends DateTimeZone {}'); } if ($class === 'MyInterval') { eval('class MyInterval extends DateInterval {}'); } if ($class === 'Later') { eval('class Later extends Base {}'); } });
class MyDate extends DateTime {
    public function getTimezone(): MyZone { return new MyZone('UTC'); }
    public function diff(DateTimeInterface $targetObject, bool $absolute = false): MyInterval { return new MyInterval('P1D'); }
}
echo "declared MyDate\n";
echo get_class((new MyDate)->getTimezone()), "\n";
class Base { public function make(): Base { return new Base; } }
class Child extends Base { public function make(): Later { return new Later; } }
echo "declared Child\n"; echo get_class((new Child)->make()), "\n";
// already loaded: no autoload needed
class Zone2 extends DateTimeZone {} class Date2 extends DateTimeImmutable { public function getTimezone(): Zone2 { return new Zone2('UTC'); } }
echo get_class((new Date2)->getTimezone()), "\n";
// really incompatible: a deprecation for the native parent, a fatal error for a PHP parent
class Date3 extends DateTime { public function getTimezone(): stdClass { return new stdClass; } }
echo "declared Date3\n";
--EXPECTF--

Deprecated: Return type of Date3::getTimezone(): stdClass should either be compatible with DateTime::getTimezone(): DateTimeZone|false, or the #[\ReturnTypeWillChange] attribute should be used to temporarily suppress the notice in %s on line %d
autoload MyZone
autoload MyInterval
declared MyDate
MyZone
autoload Later
declared Child
Later
Zone2
declared Date3
