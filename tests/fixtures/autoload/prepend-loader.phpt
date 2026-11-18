--TEST--
Prepend loader
--FILE--
<?php
spl_autoload_register(fn($name) => print("last\n")); spl_autoload_register(fn($name) => print("first\n"), prepend:true); class_exists("Unknown");
--EXPECT--
first
last
