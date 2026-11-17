--TEST--
Exceptions: engine_undefined_constant_parent
--FILE--
<?php

try { $value=MISSING_CONSTANT; } catch (Throwable $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
Error|Undefined constant "MISSING_CONSTANT"|0|3|%s
#0 {main}
