--TEST--
Explicit call already defined
--FILE--
<?php
class Wanted {} spl_autoload_register(fn($name) => print($name)); spl_autoload_call("Wanted");
--EXPECT--
Wanted