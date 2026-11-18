--TEST--
Get parent class invalid
--FILE--
<?php
try { get_parent_class("Missing"); } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECT--
get_parent_class(): Argument #1 ($object_or_class) must be an object or a valid class name, string given