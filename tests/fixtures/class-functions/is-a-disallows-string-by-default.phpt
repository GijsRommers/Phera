--TEST--
Is a disallows string by default
--FILE--
<?php
spl_autoload_register(fn($name) => print($name)); var_dump(is_a("Unknown", "stdClass"));
--EXPECT--
bool(false)
