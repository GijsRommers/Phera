--TEST--
Exceptions: engine_abstract_throwable
--FILE--
<?php
abstract class Example {}
try { $object=new Example; } catch (Throwable $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
Error|Cannot instantiate abstract class Example|0|3|%s
#0 {main}
