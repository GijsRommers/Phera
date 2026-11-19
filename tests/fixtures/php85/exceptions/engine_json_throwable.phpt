--TEST--
Exceptions: engine_json_throwable
--FILE--
<?php

try { json_decode("{",false,512,JSON_THROW_ON_ERROR); } catch (Throwable $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
JsonException|Syntax error|4|3|%s
#0 %s(3): json_decode('{', false, 512, 4194304)
#1 {main}
