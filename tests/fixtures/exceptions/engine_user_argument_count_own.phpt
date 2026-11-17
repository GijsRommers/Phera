--TEST--
Exceptions: engine_user_argument_count_own
--FILE--
<?php
function needed($number) {}
try { needed(); } catch (ArgumentCountError $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
ArgumentCountError|Too few arguments to function needed(), 0 passed in %s on line 3 and exactly 1 expected|0|2|%s
#0 %s(3): needed()
#1 {main}
