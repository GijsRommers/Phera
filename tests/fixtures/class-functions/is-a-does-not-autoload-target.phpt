--TEST--
Is a does not autoload target
--FILE--
<?php
spl_autoload_register(fn($name) => print($name)); var_dump(is_a(new stdClass, "Unknown"));
--EXPECT--
bool(false)
