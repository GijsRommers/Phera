--TEST--
Exceptions: error_reporting_zero
--FILE--
<?php
function handler($errno,$message){echo "handler";}error_reporting(0);set_error_handler("handler");trigger_error("hidden",E_USER_NOTICE);restore_error_handler();trigger_error("hidden",E_USER_NOTICE);
--EXPECTF--
handler