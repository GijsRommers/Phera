--TEST--
Compound operations check a restricted static setter before updating
--FILE--
<?php
class Sample {
    public private(set) static int $number = 1;
    public private(set) static object $object;
    static function setup() { self::$object = new stdClass(); }
}
Sample::setup();
try { Sample::$number++; }
catch (Error $error) { echo $error->getMessage(), "\n"; }
try { Sample::$number += 'string'; }
catch (Error $error) { echo $error->getMessage(), "\n"; }
Sample::$object->number = 3;
var_dump(Sample::$number, Sample::$object->number);
--EXPECT--
Cannot indirectly modify private(set) property Sample::$number from global scope
Cannot indirectly modify private(set) property Sample::$number from global scope
int(1)
int(3)
