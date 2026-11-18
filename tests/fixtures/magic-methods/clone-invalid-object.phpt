--TEST--
A scalar clone operand reports the clone parameter type
--FILE--
<?php
try { clone 1; } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECT--
clone(): Argument #1 ($object) must be of type object, int given
