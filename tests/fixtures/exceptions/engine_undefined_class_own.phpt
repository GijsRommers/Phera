--TEST--
Exceptions: engine_undefined_class_own
--FILE--
<?php

try { $object=new MissingClass; } catch (Error $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
Error|Class "MissingClass" not found|0|3|%s
#0 {main}
