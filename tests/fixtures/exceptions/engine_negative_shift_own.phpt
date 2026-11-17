--TEST--
Exceptions: engine_negative_shift_own
--FILE--
<?php

try { $value=1 << -1; } catch (ArithmeticError $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
ArithmeticError|Bit shift by negative number|0|3|%s
#0 {main}
