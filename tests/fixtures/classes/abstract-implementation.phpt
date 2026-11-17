--TEST--
Classes: abstract-implementation
--FILE--
<?php abstract class Base { abstract function answer(); } class Child extends Base { function answer() { return 42; } } echo (new Child)->answer();
--EXPECT--
42