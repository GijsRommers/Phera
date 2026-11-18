--TEST--
Unregister bound method
--FILE--
<?php
class Loader { function run($name) {} } $loader = new Loader; spl_autoload_register([$loader, "Run"]); var_dump(spl_autoload_unregister([$loader, "run"]));
--EXPECT--
bool(true)
