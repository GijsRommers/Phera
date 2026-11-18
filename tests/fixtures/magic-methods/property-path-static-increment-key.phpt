--TEST--
Static array property increment converts the key once
--FILE--
<?php
class Box { public static array $data = []; }
var_dump(Box::$data[2.5]++, Box::$data);
--EXPECTF--
Deprecated: Implicit conversion from float 2.5 to int loses precision in /private%s on line 3

Warning: Undefined array key 2 in /private%s on line 3
NULL
array(1) {
  [2]=>
  int(1)
}
