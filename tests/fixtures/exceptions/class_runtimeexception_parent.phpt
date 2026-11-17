--TEST--
Exceptions: class_runtimeexception_parent
--FILE--
<?php
try { throw new RuntimeException("failure", 17); } catch (Exception $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
RuntimeException|failure|17|2|%s
#0 {main}
