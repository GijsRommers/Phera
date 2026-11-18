--TEST--
A missing property is created before the write checks its array key
--FILE--
<?php
class Box {}
$b = new Box;
$name = [];
$b->{$name}[1.5] = 4;
var_dump($b->Array);
--EXPECTF--
Warning: Array to string conversion in %s on line 5

Deprecated: Creation of dynamic property Box::$Array is deprecated in %s on line 5

Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 5
array(1) {
  [1]=>
  int(4)
}
