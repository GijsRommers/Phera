--TEST--
Unsetting an array element inside a property converts its key once
--FILE--
<?php
class Box { public array $data = [2 => 'v']; }
$b = new Box;
unset($b->data[2.5]);
unset($b->data[3.5][4.5]);
var_dump($b->data);
--EXPECTF--
Deprecated: Implicit conversion from float 2.5 to int loses precision in %s on line 4

Deprecated: Implicit conversion from float 3.5 to int loses precision in %s on line 5
array(0) {
}
