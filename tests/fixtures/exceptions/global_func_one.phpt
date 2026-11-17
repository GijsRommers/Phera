--TEST--
Exceptions: global_func_one
--FILE--
<?php
try{func_get_arg(0);}catch(Throwable $error){echo get_class($error),"|",$error->getMessage(),"\n",$error->getTraceAsString();}
--EXPECTF--
Error|func_get_arg() cannot be called from the global scope
#0 %s(2): func_get_arg(0)
#1 {main}