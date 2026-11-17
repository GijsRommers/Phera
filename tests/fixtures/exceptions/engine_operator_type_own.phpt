--TEST--
Exceptions: engine_operator_type_own
--FILE--
<?php

try { $value=[] + 2; } catch (TypeError $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
TypeError|Unsupported operand types: array + int|0|3|%s
#0 {main}
