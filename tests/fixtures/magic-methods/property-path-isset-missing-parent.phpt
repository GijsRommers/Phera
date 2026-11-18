--TEST--
Isset converts offsets only while a property path can be indexed
--FILE--
<?php
class Box { public array $data = [1 => true]; }
$b = new Box;
var_dump(isset($b->data[1.5]), isset($b->data[2.5][3.5]));
--EXPECTF--
Deprecated: Implicit conversion from float 1.5 to int loses precision in /private%s on line 4

Deprecated: Implicit conversion from float 2.5 to int loses precision in /private%s on line 4
bool(true)
bool(false)
