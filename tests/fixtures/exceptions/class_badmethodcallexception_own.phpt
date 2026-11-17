--TEST--
Exceptions: class_badmethodcallexception_own
--FILE--
<?php
try { throw new BadMethodCallException("failure", 17); } catch (BadMethodCallException $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
BadMethodCallException|failure|17|2|%s
#0 {main}
