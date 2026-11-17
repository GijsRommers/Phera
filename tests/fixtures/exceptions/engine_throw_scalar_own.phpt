--TEST--
Exceptions: engine_throw_scalar_own
--FILE--
<?php

try { throw 7; } catch (Error $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
Error|Can only throw objects|0|3|%s
#0 {main}
