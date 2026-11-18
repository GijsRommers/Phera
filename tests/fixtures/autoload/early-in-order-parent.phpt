--TEST--
Early in order parent
--FILE--
<?php
var_dump(class_exists("Child", false)); class ParentClass {} class Child extends ParentClass {}
--EXPECT--
bool(true)
