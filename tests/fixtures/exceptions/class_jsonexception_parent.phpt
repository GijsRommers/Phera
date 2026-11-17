--TEST--
Exceptions: class_jsonexception_parent
--FILE--
<?php
try { throw new JsonException("failure", 17); } catch (Exception $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
JsonException|failure|17|2|%s
#0 {main}
