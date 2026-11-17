--TEST--
Exceptions: class_runtimeexception_own
--FILE--
<?php
try { throw new RuntimeException("failure", 17); } catch (RuntimeException $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
RuntimeException|failure|17|2|%s
#0 {main}
