--TEST--
Append during load
--FILE--
<?php
spl_autoload_register(function($name) { echo "first\n"; spl_autoload_register(fn($name) => print("appended\n")); }); class_exists("Unknown");
--EXPECT--
first
appended
