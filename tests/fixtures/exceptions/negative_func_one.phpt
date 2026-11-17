--TEST--
Exceptions: negative_func_one
--FILE--
<?php
try{func_get_arg(-1);}catch(Throwable $error){echo get_class($error),"|",$error->getMessage(),"\n",$error->getTraceAsString();}
--EXPECTF--
ValueError|func_get_arg(): Argument #1 ($position) must be greater than or equal to 0
#0 %s(2): func_get_arg(-1)
#1 {main}