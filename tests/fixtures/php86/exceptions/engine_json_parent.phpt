--TEST--
Exceptions: engine_json_parent
--FILE--
<?php

try { json_decode("{",false,512,JSON_THROW_ON_ERROR); } catch (Exception $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
JsonException|Syntax error near location 1:2|4|3|%s
#0 %s(3): json_decode('{', false, 512, 4194304)
#1 {main}
