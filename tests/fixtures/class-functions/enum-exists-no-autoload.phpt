--TEST--
Enum exists no autoload
--FILE--
<?php
spl_autoload_register(fn($name) => print($name)); var_dump(enum_exists("Example", false));
--EXPECT--
bool(false)
