--TEST--
Is callable autoloads static
--FILE--
<?php
spl_autoload_register(function($name) { echo $name; class Known { public static function run() {} } }); var_dump(is_callable("Known::run"));
--EXPECT--
Knownbool(true)
