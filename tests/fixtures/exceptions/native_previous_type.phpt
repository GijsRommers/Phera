--TEST--
Exceptions: native_previous_type
--FILE--
<?php
try{new Exception("x",0,new stdClass);}catch(TypeError $error){echo $error->getMessage(),"\n",$error->getTraceAsString();}
--EXPECTF--
Exception::__construct(): Argument #3 ($previous) must be of type ?Throwable, stdClass given
#0 %s(2): Exception->__construct('x', 0, Object(stdClass))
#1 {main}