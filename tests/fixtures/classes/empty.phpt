--TEST--
Classes: empty
--FILE--
<?php class Box {} var_dump(new Box);
--EXPECT--
object(Box)#1 (0) {
}
