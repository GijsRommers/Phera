--TEST--
Get class type error
--FILE--
<?php
try { get_class("stdClass"); } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECT--
get_class(): Argument #1 ($object) must be of type object, string given