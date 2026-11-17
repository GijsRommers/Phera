--TEST--
Exceptions: engine_readonly_own
--FILE--
<?php
class Example {public readonly int $number; function __construct(){$this->number=1;}}
try { $object=new Example; $object->number=2; } catch (Error $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
Error|Cannot modify readonly property Example::$number|0|3|%s
#0 {main}
