--TEST--
Class conformance: intrinsic-property_exists
--FILE--
<?php interface I {} class P {} class C extends P implements I {public $value=5;function hi(){return 'C';}} $object=new C;var_dump(property_exists('C','value'));
--EXPECT--
bool(true)
