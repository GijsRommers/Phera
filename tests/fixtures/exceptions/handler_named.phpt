--TEST--
Exceptions: handler_named
--FILE--
<?php
function handler($errno,$message){echo $errno,"|",$message;}set_error_handler(callback:"handler",error_levels:E_USER_WARNING);trigger_error(message:"named",error_level:E_USER_WARNING);
--EXPECTF--
512|named