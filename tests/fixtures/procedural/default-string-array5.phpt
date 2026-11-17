--TEST--
default-string-array5
--FILE--
<?php function invalid5(string $value = []) {} echo 'unreachable';
--EXPECTF--

Fatal error: Cannot use array as default value for parameter $value of type string in %s on line 1
