--TEST--
Exceptions: engine_argument_type_own
--FILE--
<?php
function typed(int $number) {}
try { typed([]); } catch (TypeError $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
TypeError|typed(): Argument #1 ($number) must be of type int, array given, called in %s on line 3|0|2|%s
#0 %s(3): typed(Array)
#1 {main}
