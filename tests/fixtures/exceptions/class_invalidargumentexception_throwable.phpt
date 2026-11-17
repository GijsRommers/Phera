--TEST--
Exceptions: class_invalidargumentexception_throwable
--FILE--
<?php
try { throw new InvalidArgumentException("failure", 17); } catch (Throwable $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
InvalidArgumentException|failure|17|2|%s
#0 {main}
