--TEST--
Class parents order
--FILE--
<?php
class A {} class B extends A {} class C extends B {} var_dump(class_parents(C::class));
--EXPECT--
array(2) {
  ["B"]=>
  string(1) "B"
  ["A"]=>
  string(1) "A"
}
