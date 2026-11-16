--TEST--
default-variable5
--FILE--
<?php function invalid5($value = $variable) {} echo 'unreachable';
--EXPECTF--

Fatal error: Constant expression contains invalid operations in %s on line 1
