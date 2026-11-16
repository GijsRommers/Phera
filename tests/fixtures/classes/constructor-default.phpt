--TEST--
Classes: constructor-default
--FILE--
<?php class Box { function __construct($value=7) { echo $value; } } new Box;
--EXPECT--
7