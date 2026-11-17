--TEST--
Exceptions: engine_argument_count_parent
--FILE--
<?php

try { intdiv(); } catch (TypeError $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
ArgumentCountError|intdiv() expects exactly 2 arguments, 0 given|0|3|%s
#0 %s(3): intdiv()
#1 {main}
