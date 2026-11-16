--TEST--
default-call5
--FILE--
<?php function invalid5($value = strlen('a')) {} echo 'unreachable';
--EXPECTF--

Fatal error: Constant expression contains invalid operations in %s on line 1
