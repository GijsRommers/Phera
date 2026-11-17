--TEST--
Exceptions: class_overflowexception_own
--FILE--
<?php
try { throw new OverflowException("failure", 17); } catch (OverflowException $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
OverflowException|failure|17|2|%s
#0 {main}
