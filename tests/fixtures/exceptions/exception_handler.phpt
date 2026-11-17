--TEST--
Exceptions: exception_handler
--FILE--
<?php
function last_resort($error){echo get_class($error),"|",$error->getMessage();}set_exception_handler("last_resort");function fail(){throw new Exception("x");}fail();
--EXPECTF--
Exception|x