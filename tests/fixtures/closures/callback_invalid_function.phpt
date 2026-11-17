--TEST--
callback invalid function
--FILE--
<?php
try{call_user_func("missingClosureFixture");}catch(TypeError $e){echo $e->getMessage(),"
";}
--EXPECT--
call_user_func(): Argument #1 ($callback) must be a valid callback, function "missingClosureFixture" not found or invalid function name
