--TEST--
Exceptions: engine_return_type_own
--FILE--
<?php
function typed(): int {return [];}
try { typed(); } catch (TypeError $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
TypeError|typed(): Return value must be of type int, array returned|0|2|%s
#0 %s(3): typed()
#1 {main}
