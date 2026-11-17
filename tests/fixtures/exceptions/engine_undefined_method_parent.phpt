--TEST--
Exceptions: engine_undefined_method_parent
--FILE--
<?php
class Example {}
try { (new Example)->missing(); } catch (Throwable $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
Error|Call to undefined method Example::missing()|0|3|%s
#0 {main}
