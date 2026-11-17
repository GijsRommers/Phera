--TEST--
Exceptions: user_error_deprecated
--FILE--
<?php
function handler($errno,$message){echo $errno,"|",$message,"\n";}set_error_handler("handler");trigger_error("fatal",E_USER_ERROR);echo "continued";
--EXPECTF--
8192|Passing E_USER_ERROR to trigger_error() is deprecated since 8.4, throw an exception or call exit with a string message instead
256|fatal
continued