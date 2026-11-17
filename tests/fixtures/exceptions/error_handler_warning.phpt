--TEST--
Exceptions: error_handler_warning
--FILE--
<?php
function handler($errno,$message,$file,$line){echo $errno,"|",$message,"|",$line;}set_error_handler("handler");echo $missing;
--EXPECTF--
2|Undefined variable $missing|2