--TEST--
Exceptions: user_warning_trace
--FILE--
<?php
function handler($errno,$message,$file,$line){throw new ErrorException($message,0,$errno,$file,$line);}set_error_handler("handler");try{trigger_error("warning",E_USER_WARNING);}catch(Throwable $error){echo $error->getTraceAsString();}
--EXPECTF--
#0 [internal function]: handler(512, 'warning', '%s...', 2)
#1 %s(2): trigger_error('warning', 512)
#2 {main}