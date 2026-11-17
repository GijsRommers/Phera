--TEST--
Exceptions: class_overflowexception_parent
--FILE--
<?php
try { throw new OverflowException("failure", 17); } catch (RuntimeException $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
OverflowException|failure|17|2|%s
#0 {main}
