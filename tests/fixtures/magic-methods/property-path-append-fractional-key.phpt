--TEST--
Nested append converts the parent array key once
--FILE--
<?php
class Box { public array $data = []; }
$b = new Box;
$b->data[3.5][] = 1;
var_dump($b->data);
--EXPECTF--
Deprecated: Implicit conversion from float 3.5 to int loses precision in %s on line 4
array(1) {
  [3]=>
  array(1) {
    [0]=>
    int(1)
  }
}
