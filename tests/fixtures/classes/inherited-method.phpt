--TEST--
Classes: inherited-method
--FILE--
<?php class Base { function answer() { return 42; } } class Child extends Base {} echo (new Child)->answer();
--EXPECT--
42