--TEST--
Reflection deprecations keep the previous property and invocation behavior
--FILE--
<?php
class Record {
    public $value;
    public static function name() { return 'record'; }
}
$other = new stdClass;
$property = new ReflectionProperty(Record::class, 'value');
$method = new ReflectionMethod(Record::class, 'name');
set_error_handler(function ($severity, $message) { echo $severity, ': ', $message, PHP_EOL; });
$property->setValue($other, 1);
var_dump($other->value);
$property->setRawValue($other, 2);
var_dump($other->value);
var_dump($method->invoke($other), $method->invokeArgs($other, []), $method->invoke(null));
--EXPECT--
int(1)
int(2)
string(6) "record"
string(6) "record"
string(6) "record"
