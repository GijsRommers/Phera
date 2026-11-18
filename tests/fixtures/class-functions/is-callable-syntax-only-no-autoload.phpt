--TEST--
Is callable syntax only no autoload
--FILE--
<?php
spl_autoload_register(fn($name) => print($name)); var_dump(is_callable(["Unknown","run"],true,$name), $name);
--EXPECT--
bool(true)
string(12) "Unknown::run"
