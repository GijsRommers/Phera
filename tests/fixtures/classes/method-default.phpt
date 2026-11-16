--TEST--
Classes: method-default
--FILE--
<?php class Box { function answer($value = 42) { return $value; } } echo (new Box)->answer();
--EXPECT--
42