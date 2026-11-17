--TEST--
Class conformance: intrinsic-interface_exists
--FILE--
<?php interface I {} class P {} class C extends P implements I {public $value=5;function hi(){return 'C';}} $object=new C;var_dump(interface_exists('I'));
--EXPECT--
bool(true)
