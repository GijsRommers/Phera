--TEST--
Exceptions: engine_throw_object_throwable
--FILE--
<?php

try { throw new stdClass; } catch (Throwable $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
Error|Cannot throw objects that do not implement Throwable|0|3|%s
#0 {main}
