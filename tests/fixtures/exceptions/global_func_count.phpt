--TEST--
Exceptions: global_func_count
--FILE--
<?php
try{func_num_args();}catch(Throwable $error){echo get_class($error),"|",$error->getMessage(),"\n",$error->getTraceAsString();}
--EXPECTF--
Error|func_num_args() must be called from a function context
#0 %s(2): func_num_args()
#1 {main}