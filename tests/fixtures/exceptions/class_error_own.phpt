--TEST--
Exceptions: class_error_own
--FILE--
<?php
try { throw new Error("failure", 17); } catch (Error $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
Error|failure|17|2|%s
#0 {main}
