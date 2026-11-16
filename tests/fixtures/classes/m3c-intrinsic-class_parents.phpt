--TEST--
Class conformance: intrinsic-class_parents
--FILE--
<?php interface I {} class P {} class C extends P implements I {public $value=5;function hi(){return 'C';}} $object=new C;var_dump(class_parents('C'));
--EXPECT--
array(1) {
  ["P"]=>
  string(1) "P"
}
