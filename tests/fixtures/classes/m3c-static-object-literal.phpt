--TEST--
Class conformance: static-object-literal
--FILE--
<?php class C {static function hi(){return 4;}} $target='C';$object=new C;$method='hi';echo $object::hi();
--EXPECT--
4
