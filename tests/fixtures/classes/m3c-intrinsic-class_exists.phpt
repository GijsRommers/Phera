--TEST--
Class conformance: intrinsic-class_exists
--FILE--
<?php interface I {} class P {} class C extends P implements I {public $value=5;function hi(){return 'C';}} $object=new C;var_dump(class_exists('P'));
--EXPECT--
bool(true)
