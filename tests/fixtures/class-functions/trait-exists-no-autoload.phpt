--TEST--
Trait exists no autoload
--FILE--
<?php
spl_autoload_register(fn($name) => print($name)); var_dump(trait_exists("Example", false));
--EXPECT--
bool(false)
