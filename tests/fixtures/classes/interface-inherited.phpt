--TEST--
Classes: interface-inherited
--FILE--
<?php interface Contract {} class Base implements Contract {} class Child extends Base {} var_dump(new Child instanceof Contract);
--EXPECT--
bool(true)
