--TEST--
Class conformance: intrinsic-is_a
--FILE--
<?php interface I {} class P {} class C extends P implements I {public $value=5;function hi(){return 'C';}} $object=new C;var_dump(is_a($object,'P'));
--EXPECT--
bool(true)
