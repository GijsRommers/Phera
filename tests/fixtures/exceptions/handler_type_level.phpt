--TEST--
Exceptions: handler_type_level
--FILE--
<?php
try{trigger_error("x",[]);}catch(TypeError $error){echo $error->getMessage();}
--EXPECTF--
trigger_error(): Argument #2 ($error_level) must be of type int, array given