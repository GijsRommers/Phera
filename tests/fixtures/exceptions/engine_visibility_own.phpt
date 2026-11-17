--TEST--
Exceptions: engine_visibility_own
--FILE--
<?php
class Example {private function hidden(){}}
try { (new Example)->hidden(); } catch (Error $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
Error|Call to private method Example::hidden() from global scope|0|3|%s
#0 {main}
