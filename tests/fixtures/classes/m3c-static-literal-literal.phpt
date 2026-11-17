--TEST--
Class conformance: static-literal-literal
--FILE--
<?php class C {static function hi(){return 4;}} $target='C';$object=new C;$method='hi';echo C::hi();
--EXPECT--
4
