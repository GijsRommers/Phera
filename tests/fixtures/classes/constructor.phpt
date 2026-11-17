--TEST--
Classes: constructor
--FILE--
<?php class Box { function __construct($message) { echo $message; } } var_dump(new Box("hello"));
--EXPECT--
helloobject(Box)#1 (0) {
}
