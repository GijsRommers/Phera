--TEST--
Class implements inherited order
--FILE--
<?php
interface I {} interface J {} class A implements I {} class B extends A implements J {} var_dump(class_implements(B::class));
--EXPECT--
array(2) {
  ["I"]=>
  string(1) "I"
  ["J"]=>
  string(1) "J"
}
