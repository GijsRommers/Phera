--TEST--
Get class vars defaults
--FILE--
<?php
class A { static $value = 3; } A::$value=8; var_dump(get_class_vars(A::class));
--EXPECT--
array(1) {
  ["value"]=>
  int(3)
}
