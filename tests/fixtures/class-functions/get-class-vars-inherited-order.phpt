--TEST--
Get class vars inherited order
--FILE--
<?php
class A { public $first=1; } class B extends A { public $second=2; } var_dump(get_class_vars(B::class));
--EXPECT--
array(2) {
  ["second"]=>
  int(2)
  ["first"]=>
  int(1)
}
