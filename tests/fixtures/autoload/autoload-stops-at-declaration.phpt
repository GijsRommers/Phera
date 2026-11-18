--TEST--
Autoload stops at declaration
--FILE--
<?php
spl_autoload_register(function ($name) { class Wanted {} }); spl_autoload_register(fn($name) => print("unexpected\n")); var_dump(class_exists("Wanted"));
--EXPECT--
bool(true)
