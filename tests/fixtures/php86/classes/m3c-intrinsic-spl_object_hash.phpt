--TEST--
Class conformance: intrinsic-spl_object_hash
--FILE--
<?php interface I {} class P {} class C extends P implements I {public $value=5;function hi(){return 'C';}} $object=new C;var_dump(spl_object_hash($object));
--EXPECTF--

Deprecated: Function spl_object_hash() is deprecated since 8.6, consider using spl_object_id() instead in %s on line 1
string(32) "00000000000000010000000000000000"
