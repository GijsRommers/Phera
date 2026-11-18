--TEST--
Late forward parent
--FILE--
<?php
var_dump(class_exists("Child", false)); class Child extends ParentClass {} class ParentClass {} var_dump(class_exists("Child", false));
--EXPECT--
bool(false)
bool(true)
