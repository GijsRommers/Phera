--TEST--
Exceptions: user_error_fatal
--FILE--
<?php
trigger_error("fatal",E_USER_ERROR);echo "bad";
--EXPECTF--

Deprecated: Passing E_USER_ERROR to trigger_error() is deprecated since 8.4, throw an exception or call exit with a string message instead in %s on line 2

Fatal error: fatal in %s on line 2
