--TEST--
Classes: method-scalar-type
--FILE--
<?php class Box { function answer(int $number): int { return $number; } } echo (new Box)->answer("42");
--EXPECT--
42