--TEST--
Invalid name does not call
--FILE--
<?php
spl_autoload_register(fn($name) => print($name)); var_dump(class_exists("not a class"));
--EXPECT--
bool(false)
