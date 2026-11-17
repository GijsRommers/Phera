--TEST--
Class conformance: intrinsic-spl_object_id
--FILE--
<?php interface I {} class P {} class C extends P implements I {public $value=5;function hi(){return 'C';}} $object=new C;var_dump(spl_object_id($object));
--EXPECT--
int(1)
