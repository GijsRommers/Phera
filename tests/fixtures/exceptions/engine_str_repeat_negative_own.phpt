--TEST--
Exceptions: engine_str_repeat_negative_own
--FILE--
<?php

try { str_repeat("x",-1); } catch (ValueError $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
ValueError|str_repeat(): Argument #2 ($times) must be greater than or equal to 0|0|3|%s
#0 %s(3): str_repeat('x', -1)
#1 {main}
