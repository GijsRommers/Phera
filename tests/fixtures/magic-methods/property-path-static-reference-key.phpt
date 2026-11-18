--TEST--
Taking a reference to a static property array converts the key once
--FILE--
<?php
class Box { public static array $data = []; }
$r =& Box::$data[2.5];
$r = 4;
var_dump(Box::$data);
--EXPECTF--
Deprecated: Implicit conversion from float 2.5 to int loses precision in %s on line 3
array(1) {
  [2]=>
  &int(4)
}
