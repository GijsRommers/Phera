--TEST--
Classes: method-named
--FILE--
<?php class Box { function sub($left, $right) { return $left - $right; } } echo (new Box)->sub(right: 2, left: 9);
--EXPECT--
7