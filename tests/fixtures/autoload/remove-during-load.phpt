--TEST--
Remove during load
--FILE--
<?php
$skip = fn($name) => print("unexpected\n"); spl_autoload_register(function($name) use ($skip) { echo "first\n"; spl_autoload_unregister($skip); }); spl_autoload_register($skip); class_exists("Unknown");
--EXPECT--
first
