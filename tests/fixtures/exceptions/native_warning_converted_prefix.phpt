--TEST--
Exceptions: native_warning_converted_prefix
--FILE--
<?php
function handler($errno,$message,$file,$line){throw new ErrorException($message,0,$errno,$file,$line);}set_error_handler("handler");try{str_repeat(7,2.5);}catch(Throwable $error){echo $error->getTraceAsString();}
--EXPECTF--
#0 [internal function]: handler(8192, 'Implicit conver...', '%s...', 2)
#1 %s(2): str_repeat('7', 2.5)
#2 {main}