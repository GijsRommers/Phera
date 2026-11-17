--TEST--
Exceptions: error_handler_throw
--FILE--
<?php
function handler($errno,$message,$file,$line){throw new ErrorException($message,0,$errno,$file,$line);}set_error_handler("handler");try{trigger_error("warning",E_USER_WARNING);}catch(ErrorException $error){echo $error->getMessage(),"|",$error->getSeverity(),"|",$error->getLine();}
--EXPECTF--
warning|512|2