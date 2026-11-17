--TEST--
Exceptions: class_rangeexception_own
--FILE--
<?php
try { throw new RangeException("failure", 17); } catch (RangeException $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
RangeException|failure|17|2|%s
#0 {main}
