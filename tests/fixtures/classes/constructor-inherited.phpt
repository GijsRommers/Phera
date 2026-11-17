--TEST--
Classes: constructor-inherited
--FILE--
<?php class Base { function __construct($value) { echo $value; } } class Child extends Base {} new Child(7);
--EXPECT--
7