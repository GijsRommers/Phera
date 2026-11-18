--TEST--
A nested array write retains its key when the receiver comes from an array
--FILE--
<?php
class Box { public array $data = []; }
$b = new Box;
$arr = ['o' => $b];
$arr['o']->data[7.5] = 1;
var_dump($b->data);
--EXPECTF--
Deprecated: Implicit conversion from float 7.5 to int loses precision in %s on line 5
array(1) {
  [7]=>
  int(1)
}
