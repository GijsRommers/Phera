--TEST--
Classes: method-arguments
--FILE--
<?php class Box { function add($left, $right) { return $left + $right; } } echo (new Box)->add(2, 3);
--EXPECT--
5