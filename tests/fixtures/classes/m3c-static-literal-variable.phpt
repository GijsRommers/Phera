--TEST--
Class conformance: static-literal-variable
--FILE--
<?php class C {static function hi(){return 4;}} $target='C';$object=new C;$method='hi';echo C::$method();
--EXPECT--
4
