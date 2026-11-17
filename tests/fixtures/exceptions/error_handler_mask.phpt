--TEST--
Exceptions: error_handler_mask
--FILE--
<?php
function handler($errno,$message){echo "handler:";return true;}set_error_handler("handler",E_USER_WARNING);trigger_error("notice",E_USER_NOTICE);trigger_error("warning",E_USER_WARNING);
--EXPECTF--

Notice: notice in %s on line 2
handler: