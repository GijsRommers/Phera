--TEST--
default-bool-int5
--FILE--
<?php function invalid5(bool $value = 1) {} echo 'unreachable';
--EXPECTF--

Fatal error: Cannot use int as default value for parameter $value of type bool in %s on line 1
