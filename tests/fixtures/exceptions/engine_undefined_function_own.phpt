--TEST--
Exceptions: engine_undefined_function_own
--FILE--
<?php

try { missing_function(); } catch (Error $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
Error|Call to undefined function missing_function()|0|3|%s
#0 {main}
