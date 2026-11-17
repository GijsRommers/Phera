--TEST--
Exceptions: class_lengthexception_parent
--FILE--
<?php
try { throw new LengthException("failure", 17); } catch (LogicException $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
LengthException|failure|17|2|%s
#0 {main}
