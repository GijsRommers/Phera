--TEST--
Class conformance: dynamic-method
--FILE--
<?php class C {function hi(){return 4;}} $name='hi';echo (new C)->$name();
--EXPECT--
4
