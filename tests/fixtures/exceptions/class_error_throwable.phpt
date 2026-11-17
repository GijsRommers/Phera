--TEST--
Exceptions: class_error_throwable
--FILE--
<?php
try { throw new Error("failure", 17); } catch (Throwable $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
Error|failure|17|2|%s
#0 {main}
