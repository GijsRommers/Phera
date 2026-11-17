--TEST--
Exceptions: exception_handler_restore
--FILE--
<?php
function first($error){echo "first";}function second($error){echo "second";}var_dump(set_exception_handler("first"));var_dump(set_exception_handler("second"));restore_exception_handler();throw new Exception("x");
--EXPECTF--
NULL
string(5) "first"
first