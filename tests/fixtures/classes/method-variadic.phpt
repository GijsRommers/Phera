--TEST--
Classes: method-variadic
--FILE--
<?php class Box { function add(...$values) { return array_sum($values); } } echo (new Box)->add(1,2,3);
--EXPECT--
6