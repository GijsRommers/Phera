--TEST--
Increment reports each property array key conversion once and in order
--FILE--
<?php
class Box { public array $data = []; }
$b = new Box;
var_dump($b->data[4.5][1.5]++);
var_dump($b->data);
--EXPECTF--
Deprecated: Implicit conversion from float 4.5 to int loses precision in /private%s on line 4

Warning: Undefined array key 4 in /private%s on line 4

Deprecated: Implicit conversion from float 1.5 to int loses precision in /private%s on line 4

Warning: Undefined array key 1 in /private%s on line 4
NULL
array(1) {
  [4]=>
  array(1) {
    [1]=>
    int(1)
  }
}
