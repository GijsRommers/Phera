--TEST--
Exceptions: error_reporting_type
--FILE--
<?php
try{error_reporting([]);}catch(TypeError $error){echo $error->getMessage();}
--EXPECTF--
error_reporting(): Argument #1 ($error_level) must be of type ?int, array given