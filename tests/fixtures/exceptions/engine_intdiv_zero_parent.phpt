--TEST--
Exceptions: engine_intdiv_zero_parent
--FILE--
<?php

try { intdiv(1,0); } catch (ArithmeticError $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
DivisionByZeroError|Division by zero|0|3|%s
#0 %s(3): intdiv(1, 0)
#1 {main}
