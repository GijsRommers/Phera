--TEST--
Exceptions: class_jsonexception_throwable
--FILE--
<?php
try { throw new JsonException("failure", 17); } catch (Throwable $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
JsonException|failure|17|2|%s
#0 {main}
