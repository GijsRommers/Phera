--TEST--
Get class vars invalid
--FILE--
<?php
try { get_class_vars("Missing"); } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECT--
get_class_vars(): Argument #1 ($class) must be a valid class name, Missing given