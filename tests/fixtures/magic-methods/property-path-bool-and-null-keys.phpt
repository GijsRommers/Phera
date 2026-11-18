--TEST--
A PHP property array write handles numeric strings, boolean and null keys
--FILE--
<?php
class Box { public array $data = []; }
$b = new Box;
$b->data["8"] = 1;
$b->data[true] = 2;
$b->data[null] = 3;
var_dump(array_keys($b->data));
--EXPECTF--
Deprecated: Using null as an array offset is deprecated, use an empty string instead in /private%s on line 6
array(3) {
  [0]=>
  int(8)
  [1]=>
  int(1)
  [2]=>
  string(0) ""
}
