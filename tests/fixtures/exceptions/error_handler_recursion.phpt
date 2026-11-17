--TEST--
Exceptions: error_handler_recursion
--FILE--
<?php
function handler($errno,$message){echo "handler";trigger_error("nested",E_USER_NOTICE);}set_error_handler("handler");trigger_error("outer",E_USER_NOTICE);
--EXPECTF--
handler
Notice: nested in %s on line 2
