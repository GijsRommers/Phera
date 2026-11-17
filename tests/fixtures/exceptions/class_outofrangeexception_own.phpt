--TEST--
Exceptions: class_outofrangeexception_own
--FILE--
<?php
try { throw new OutOfRangeException("failure", 17); } catch (OutOfRangeException $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
OutOfRangeException|failure|17|2|%s
#0 {main}
