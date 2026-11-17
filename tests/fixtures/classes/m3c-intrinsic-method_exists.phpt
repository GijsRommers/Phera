--TEST--
Class conformance: intrinsic-method_exists
--FILE--
<?php interface I {} class P {} class C extends P implements I {public $value=5;function hi(){return 'C';}} $object=new C;var_dump(method_exists('C','hi'));
--EXPECT--
bool(true)
