--TEST--
Exceptions: class_argumentcounterror_parent
--FILE--
<?php
try { throw new ArgumentCountError("failure", 17); } catch (TypeError $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
ArgumentCountError|failure|17|2|%s
#0 {main}
