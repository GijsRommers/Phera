--TEST--
Get class methods hierarchy order
--FILE--
<?php
class A { public function first() {} } class B extends A { public function second() {} } var_dump(get_class_methods(new B));
--EXPECT--
array(2) {
  [0]=>
  string(6) "second"
  [1]=>
  string(5) "first"
}
