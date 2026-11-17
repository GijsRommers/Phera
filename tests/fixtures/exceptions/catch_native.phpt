--TEST--
Exceptions: catch_native
--FILE--
<?php
try { intdiv(1,0); } catch(DivisionByZeroError $error) { echo $error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"\n",$error->getTraceAsString(); }
--EXPECTF--
Division by zero|0|2
#0 %s(2): intdiv(1, 0)
#1 {main}