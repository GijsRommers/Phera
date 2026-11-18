--TEST--
Get class methods invalid
--FILE--
<?php
try { get_class_methods("Missing"); } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECT--
get_class_methods(): Argument #1 ($object_or_class) must be an object or a valid class name, string given