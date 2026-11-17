--TEST--
Class conformance: static-object-variable
--FILE--
<?php class C {static function hi(){return 4;}} $target='C';$object=new C;$method='hi';echo $object::$method();
--EXPECT--
4
