--TEST--
Exceptions: class_errorexception_own
--FILE--
<?php
try { throw new ErrorException("failure", 17); } catch (ErrorException $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
ErrorException|failure|17|2|%s
#0 {main}
