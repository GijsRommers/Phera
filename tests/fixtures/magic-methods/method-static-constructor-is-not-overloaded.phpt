--TEST--
Static calls to missing constructors bypass __callStatic
--FILE--
<?php
class Test { static function __callStatic($name, $args) { echo $name; } }
try { Test::__construct(); } catch (Error $e) { echo $e->getMessage(); }
var_dump(is_callable([Test::class, '__construct']));
--EXPECT--
Cannot call constructorbool(false)
