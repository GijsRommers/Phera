--TEST--
Class conformance: intrinsic-get_class
--FILE--
<?php interface I {} class P {} class C extends P implements I {public $value=5;function hi(){return 'C';}} $object=new C;var_dump(get_class($object));
--EXPECT--
string(1) "C"
