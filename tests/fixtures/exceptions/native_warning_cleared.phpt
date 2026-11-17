--TEST--
Exceptions: native_warning_cleared
--FILE--
<?php
function handler($errno,$message){throw new ErrorException($message);}set_error_handler("handler");try{str_repeat(null,[]);}catch(Throwable){}restore_error_handler();try{$value=1/0;}catch(Throwable $error){echo $error->getTraceAsString();}
--EXPECTF--
#0 {main}