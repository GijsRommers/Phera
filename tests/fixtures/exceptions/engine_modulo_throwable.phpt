--TEST--
Exceptions: engine_modulo_throwable
--FILE--
<?php

try { $value=1%0; } catch (Throwable $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
DivisionByZeroError|Modulo by zero|0|3|%s
#0 {main}
