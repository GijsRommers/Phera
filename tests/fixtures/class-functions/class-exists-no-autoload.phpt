--TEST--
Class exists no autoload
--FILE--
<?php
spl_autoload_register(fn($name) => print($name)); var_dump(class_exists("Example", false));
--EXPECT--
bool(false)
