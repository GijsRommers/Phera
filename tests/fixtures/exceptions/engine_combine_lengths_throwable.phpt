--TEST--
Exceptions: engine_combine_lengths_throwable
--FILE--
<?php

try { array_combine([1],[2,3]); } catch (Throwable $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
ValueError|array_combine(): Argument #1 ($keys) and argument #2 ($values) must have the same number of elements|0|3|%s
#0 %s(3): array_combine(Array, Array)
#1 {main}
