--TEST--
Exceptions: handler_type_message
--FILE--
<?php
try{trigger_error([]);}catch(TypeError $error){echo $error->getMessage();}
--EXPECTF--
trigger_error(): Argument #1 ($message) must be of type string, array given