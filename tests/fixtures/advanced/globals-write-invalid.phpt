--TEST--
Whole GLOBALS assignment is rejected before execution
--FILE--
<?php
function invalid() { $GLOBALS = []; }
--EXPECTF--

Fatal error: $GLOBALS can only be modified using the $GLOBALS[$name] = $value syntax in %s on line 2
