--TEST--
Exceptions: class_exception_parent
--FILE--
<?php
try { throw new Exception("failure", 17); } catch (Throwable $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
Exception|failure|17|2|%s
#0 {main}
