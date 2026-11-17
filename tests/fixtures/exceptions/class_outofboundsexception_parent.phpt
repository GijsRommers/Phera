--TEST--
Exceptions: class_outofboundsexception_parent
--FILE--
<?php
try { throw new OutOfBoundsException("failure", 17); } catch (RuntimeException $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
OutOfBoundsException|failure|17|2|%s
#0 {main}
