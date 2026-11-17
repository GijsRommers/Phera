--TEST--
Exceptions: engine_match_throwable
--FILE--
<?php

try { $value=match(2){1=>1}; } catch (Throwable $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
UnhandledMatchError|Unhandled match case 2|0|3|%s
#0 {main}
