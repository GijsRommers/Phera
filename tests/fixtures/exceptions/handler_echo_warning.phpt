--TEST--
Exceptions: handler_echo_warning
--FILE--
<?php
function handler($errno,$message){throw new ErrorException($message,0,$errno);}set_error_handler("handler");try{echo [];}catch(Throwable $error){echo "caught:",$error->getMessage();}
--EXPECTF--
Arraycaught:Array to string conversion