--TEST--
Classes: interface-empty
--FILE--
<?php interface Contract {} class Box implements Contract {} var_dump(new Box instanceof Contract);
--EXPECT--
bool(true)
