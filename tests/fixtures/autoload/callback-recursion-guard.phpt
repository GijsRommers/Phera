--TEST--
Callback recursion guard
--FILE--
<?php
spl_autoload_register(function($name) { echo $name, "\n"; var_dump(class_exists(strtolower($name))); }); var_dump(class_exists("Wanted"));
--EXPECT--
Wanted
bool(false)
bool(false)
