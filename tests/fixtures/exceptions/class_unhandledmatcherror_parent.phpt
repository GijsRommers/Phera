--TEST--
Exceptions: class_unhandledmatcherror_parent
--FILE--
<?php
try { throw new UnhandledMatchError("failure", 17); } catch (Error $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
UnhandledMatchError|failure|17|2|%s
#0 {main}
