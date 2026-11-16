--TEST--
Classes: constructor-named
--FILE--
<?php class Box { function __construct($first, $second) { echo $first,$second; } } new Box(second:2,first:1);
--EXPECT--
12