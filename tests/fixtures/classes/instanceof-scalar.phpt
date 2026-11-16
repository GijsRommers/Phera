--TEST--
Classes: instanceof-scalar
--FILE--
<?php var_dump(1 instanceof Missing);
--EXPECT--
bool(false)
