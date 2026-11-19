--TEST--
Reading and writing an incomplete object follow PHP diagnostics
--FILE--
<?php
$value = unserialize('O:4:"Gone":1:{s:1:"x";i:2;}');
var_dump($value->x, isset($value->x), property_exists($value, 'x'));
try { $value->x = 3; } catch (Error $error) { echo $error->getMessage(), "\n"; }
?>
--EXPECTF--

Warning: main(): The script tried to access a property on an incomplete object. Please ensure that the class definition "Gone" of the object you are trying to operate on was loaded _before_ unserialize() gets called or provide an autoloader to load the class definition in %s on line 3

Warning: main(): The script tried to access a property on an incomplete object. Please ensure that the class definition "Gone" of the object you are trying to operate on was loaded _before_ unserialize() gets called or provide an autoloader to load the class definition in %s on line 3

Warning: property_exists(): The script tried to access a property on an incomplete object. Please ensure that the class definition "Gone" of the object you are trying to operate on was loaded _before_ unserialize() gets called or provide an autoloader to load the class definition in %s on line 3
NULL
bool(false)
bool(false)
The script tried to modify a property on an incomplete object. Please ensure that the class definition "Gone" of the object you are trying to operate on was loaded _before_ unserialize() gets called or provide an autoloader to load the class definition
