--TEST--
Exceptions: class_arithmeticerror_parent
--FILE--
<?php
try { throw new ArithmeticError("failure", 17); } catch (Error $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
ArithmeticError|failure|17|2|%s
#0 {main}
