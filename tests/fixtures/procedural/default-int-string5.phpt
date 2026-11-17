--TEST--
default-int-string5
--FILE--
<?php function invalid5(int $value = 'text') {} echo 'unreachable';
--EXPECTF--

Fatal error: Cannot use string as default value for parameter $value of type int in %s on line 1
