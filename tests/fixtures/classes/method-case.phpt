--TEST--
Classes: method-case
--FILE--
<?php class Box { function answer() { return 42; } } echo (new Box)->ANSWER();
--EXPECT--
42