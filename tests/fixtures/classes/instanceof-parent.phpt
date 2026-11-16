--TEST--
Classes: instanceof-parent
--FILE--
<?php class Base {} class Child extends Base {} var_dump(new Child instanceof Base, new Base instanceof Child);
--EXPECT--
bool(true)
bool(false)
