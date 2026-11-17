--TEST--
Exceptions: native_warning_valid_trace
--FILE--
<?php
function handler($errno,$message,$file,$line){throw new ErrorException($message,0,$errno,$file,$line);}set_error_handler("handler");try{str_repeat(null,2);}catch(Throwable $error){echo $error->getTraceAsString();}
--EXPECTF--
#0 [internal function]: handler(8192, 'str_repeat(): P...', '%s...', 2)
#1 %s(2): str_repeat(NULL, 2)
#2 {main}