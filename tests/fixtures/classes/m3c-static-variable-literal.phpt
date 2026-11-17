--TEST--
Class conformance: static-variable-literal
--FILE--
<?php class C {static function hi(){return 4;}} $target='C';$object=new C;$method='hi';echo $target::hi();
--EXPECT--
4
