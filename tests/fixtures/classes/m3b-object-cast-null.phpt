--TEST--
Class members: object-cast-null
--FILE--
<?php var_dump((object)null);
--EXPECT--
object(stdClass)#1 (0) {
}
