--TEST--
Exceptions: global_func_all
--FILE--
<?php
try{func_get_args();}catch(Throwable $error){echo get_class($error),"|",$error->getMessage(),"\n",$error->getTraceAsString();}
--EXPECTF--
Error|func_get_args() cannot be called from the global scope
#0 %s(2): func_get_args()
#1 {main}