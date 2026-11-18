--TEST--
Compound assignment reports each key conversion before its undefined index warning
--FILE--
<?php
class Box { public array $data = []; }
$b = new Box;
var_dump($b->data[3.5][2.5] += 1);
var_dump($b->data);
--EXPECTF--
Deprecated: Implicit conversion from float 3.5 to int loses precision in %s on line 4

Warning: Undefined array key 3 in %s on line 4

Deprecated: Implicit conversion from float 2.5 to int loses precision in %s on line 4

Warning: Undefined array key 2 in %s on line 4
int(1)
array(1) {
  [3]=>
  array(1) {
    [2]=>
    int(1)
  }
}
