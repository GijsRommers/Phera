--TEST--
Exceptions: engine_property_type_parent
--FILE--
<?php
class Typed {public int $number;}
try { $object=new Typed; $object->number=[]; } catch (Error $error) { echo get_class($error),"|",$error->getMessage(),"|",$error->getCode(),"|",$error->getLine(),"|",basename($error->getFile()),"\n",$error->getTraceAsString(),"\n"; }
--EXPECTF--
TypeError|Cannot assign array to property Typed::$number of type int|0|3|%s
#0 {main}
