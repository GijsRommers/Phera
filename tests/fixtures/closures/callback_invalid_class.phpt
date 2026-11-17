--TEST--
callback invalid class
--FILE--
<?php
try{call_user_func(["MissingClosureFixture","x"]);}catch(TypeError $e){echo $e->getMessage(),"
";}
--EXPECT--
call_user_func(): Argument #1 ($callback) must be a valid callback, class "MissingClosureFixture" not found
