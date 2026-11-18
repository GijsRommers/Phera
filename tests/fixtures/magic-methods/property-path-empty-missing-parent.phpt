--TEST--
Empty leaves a missing parent dimension quiet
--FILE--
<?php
class Box { public array $data = ['' => true]; }
$b = new Box;
var_dump(empty($b->data[null]), empty($b->data[2.5][3.5]));
--EXPECTF--
Deprecated: Using null as an array offset is deprecated, use an empty string instead in %s on line 4

Deprecated: Implicit conversion from float 2.5 to int loses precision in %s on line 4
bool(false)
bool(true)
