--TEST--
Binding a property array element returns the bound value without reading the key again
--FILE--
<?php
class Box { public array $data = []; }
$b = new Box;
$r = 3;
var_dump($b->data[6.5][1.5] =& $r);
$r = 4;
var_dump($b->data);
--EXPECTF--
Deprecated: Implicit conversion from float 6.5 to int loses precision in /private%s on line 5

Deprecated: Implicit conversion from float 1.5 to int loses precision in /private%s on line 5
int(3)
array(1) {
  [6]=>
  array(1) {
    [1]=>
    &int(4)
  }
}
