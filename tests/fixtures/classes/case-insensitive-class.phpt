--TEST--
Classes: case-insensitive-class
--FILE--
<?php class Example {} var_dump(new EXAMPLE instanceof example);
--EXPECT--
bool(true)
