--TEST--
Exceptions: native_type_trace
--FILE--
<?php
try {str_repeat([],2);} catch(Throwable $error){echo $error->getTraceAsString();}
--EXPECTF--
#0 %s(2): str_repeat(Array, 2)
#1 {main}