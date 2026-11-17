--TEST--
Exceptions: handler_mode_callback
--FILE--
<?php
function handler($errno,$message){throw new ErrorException($message);}call_user_func("set_error_handler","handler");try{echo $missing;}catch(Throwable $error){echo $error->getMessage();}
--EXPECTF--
Undefined variable $missing