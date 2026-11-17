--TEST--
Exceptions: user_error_alias
--FILE--
<?php
function handler($errno,$message){echo $errno,"|",$message;}set_error_handler("handler");user_error("alias");
--EXPECTF--
1024|alias