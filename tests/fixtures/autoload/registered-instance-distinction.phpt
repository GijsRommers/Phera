--TEST--
Registered instance distinction
--FILE--
<?php
class Loader { function run($name) {} } $a = new Loader; $b = new Loader; spl_autoload_register([$a, "run"]); var_dump(spl_autoload_unregister([$b, "run"]));
--EXPECT--
bool(false)
