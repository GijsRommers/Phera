--TEST--
Get class vars uninitialized
--FILE--
<?php
class A { public int $value; public static int $other; } var_dump(get_class_vars(A::class));
--EXPECT--
array(2) {
  ["value"]=>
  NULL
  ["other"]=>
  NULL
}
