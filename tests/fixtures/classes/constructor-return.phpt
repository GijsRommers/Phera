--TEST--
Classes: constructor-return
--FILE--
<?php class Box { function __construct() { return 42; } } var_dump(new Box);
--EXPECT--
object(Box)#1 (0) {
}
