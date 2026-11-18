--TEST--
Taking a reference to a property array element converts its key once
--FILE--
<?php
class Box { public array $data = []; }
$b = new Box;
$r =& $b->data[5.5][1.5];
$r = 4;
var_dump($b->data);
--EXPECTF--
Deprecated: Implicit conversion from float 5.5 to int loses precision in /private%s on line 4

Deprecated: Implicit conversion from float 1.5 to int loses precision in /private%s on line 4
array(1) {
  [5]=>
  array(1) {
    [1]=>
    &int(4)
  }
}
