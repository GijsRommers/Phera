--TEST--
Classes: instanceof-unknown
--FILE--
<?php class Box {} var_dump(new Box instanceof Missing);
--EXPECT--
bool(false)
