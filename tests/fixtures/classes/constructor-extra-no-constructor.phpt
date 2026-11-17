--TEST--
Classes: constructor-extra-no-constructor
--FILE--
<?php class Box {} var_dump(new Box(1,2));
--EXPECT--
object(Box)#1 (0) {
}
