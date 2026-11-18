--TEST--
Class implements interface ancestry
--FILE--
<?php
interface I {} interface J extends I {} class A implements J {} var_dump(class_implements(A::class));
--EXPECT--
array(2) {
  ["J"]=>
  string(1) "J"
  ["I"]=>
  string(1) "I"
}
