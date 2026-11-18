--TEST--
Get class methods visibility
--FILE--
<?php
class A { private function hidden() {} public function yes() {} } var_dump(get_class_methods(A::class));
--EXPECT--
array(1) {
  [0]=>
  string(3) "yes"
}
