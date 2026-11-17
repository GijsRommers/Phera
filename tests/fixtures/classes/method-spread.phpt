--TEST--
Classes: method-spread
--FILE--
<?php class Box { function add($first,$second) { return $first+$second; } } echo (new Box)->add(...[4,5]);
--EXPECT--
9