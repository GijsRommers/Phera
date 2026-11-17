--TEST--
Class conformance: intrinsic-is_subclass_of
--FILE--
<?php interface I {} class P {} class C extends P implements I {public $value=5;function hi(){return 'C';}} $object=new C;var_dump(is_subclass_of($object,'P'));
--EXPECT--
bool(true)
