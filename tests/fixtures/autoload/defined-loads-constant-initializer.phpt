--TEST--
Defined loads constant initializer
--FILE--
<?php
class A { const X = Dependency::Y; }
spl_autoload_register(function($name) { echo $name, "\n"; class Dependency { const Y=7; } });
var_dump(defined("A::X"));
--EXPECT--
Dependency
bool(true)
