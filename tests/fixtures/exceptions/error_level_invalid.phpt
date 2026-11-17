--TEST--
Exceptions: error_level_invalid
--FILE--
<?php
try{trigger_error("bad",1);}catch(ValueError $error){echo $error->getMessage();}
--EXPECTF--
trigger_error(): Argument #2 ($error_level) must be one of E_USER_ERROR, E_USER_WARNING, E_USER_NOTICE, or E_USER_DEPRECATED