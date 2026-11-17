--TEST--
Exceptions: class_badfunctioncallexception_throwable
--FILE--
<?php
try { throw new BadFunctionCallException("failure", 17); } catch (Throwable $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
BadFunctionCallException|failure|17|2|%s
#0 {main}
