--TEST--
Exceptions: class_unexpectedvalueexception_own
--FILE--
<?php
try { throw new UnexpectedValueException("failure", 17); } catch (UnexpectedValueException $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
UnexpectedValueException|failure|17|2|%s
#0 {main}
