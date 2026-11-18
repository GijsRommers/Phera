--TEST--
Implements loads interface
--FILE--
<?php
spl_autoload_register(function($name) { echo $name, "\n"; interface Wanted {} }); class Child implements Wanted {} var_dump(interface_exists("Wanted", false));
--EXPECT--
Wanted
bool(true)
