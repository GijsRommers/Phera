--TEST--
Class conformance: static-variable-variable
--FILE--
<?php class C {static function hi(){return 4;}} $target='C';$object=new C;$method='hi';echo $target::$method();
--EXPECT--
4
