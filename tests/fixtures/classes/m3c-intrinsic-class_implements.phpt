--TEST--
Class conformance: intrinsic-class_implements
--FILE--
<?php interface I {} class P {} class C extends P implements I {public $value=5;function hi(){return 'C';}} $object=new C;var_dump(class_implements('C'));
--EXPECT--
array(1) {
  ["I"]=>
  string(1) "I"
}
