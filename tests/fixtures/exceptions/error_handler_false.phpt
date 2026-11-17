--TEST--
Exceptions: error_handler_false
--FILE--
<?php
function handler($errno,$message){echo "handler\n";return false;}set_error_handler("handler");trigger_error("warning",E_USER_WARNING);
--EXPECTF--
handler

Warning: warning in %s on line 2
