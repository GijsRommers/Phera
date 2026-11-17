--TEST--
Exceptions: class_assertionerror_parent
--FILE--
<?php
try { throw new AssertionError("failure", 17); } catch (Error $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
AssertionError|failure|17|2|%s
#0 {main}
