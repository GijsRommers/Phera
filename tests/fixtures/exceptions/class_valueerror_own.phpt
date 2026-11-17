--TEST--
Exceptions: class_valueerror_own
--FILE--
<?php
try { throw new ValueError("failure", 17); } catch (ValueError $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
ValueError|failure|17|2|%s
#0 {main}
