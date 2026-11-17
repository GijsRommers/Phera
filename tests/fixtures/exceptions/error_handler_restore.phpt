--TEST--
Exceptions: error_handler_restore
--FILE--
<?php
function first($errno,$message){echo "first";}function second($errno,$message){echo "second";}set_error_handler("first");set_error_handler("second");restore_error_handler();trigger_error("warning",E_USER_WARNING);
--EXPECTF--
first