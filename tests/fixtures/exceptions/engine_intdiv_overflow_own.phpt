--TEST--
Exceptions: engine_intdiv_overflow_own
--FILE--
<?php

try { intdiv(PHP_INT_MIN,-1); } catch (ArithmeticError $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
ArithmeticError|Division of PHP_INT_MIN by -1 is not an integer|0|3|%s
#0 %s(3): intdiv(-9223372036854775808, -1)
#1 {main}
