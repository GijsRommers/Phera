--TEST--
callback invalid array
--FILE--
<?php
foreach([[],[1,"x"],["stdClass",1],["stdClass","x"]]as $f){try{call_user_func($f);}catch(TypeError $e){echo $e->getMessage(),"
";}}
--EXPECT--
call_user_func(): Argument #1 ($callback) must be a valid callback, array callback must have exactly two members
call_user_func(): Argument #1 ($callback) must be a valid callback, first array member is not a valid class name or object
call_user_func(): Argument #1 ($callback) must be a valid callback, second array member is not a valid method
call_user_func(): Argument #1 ($callback) must be a valid callback, class stdClass does not have a method "x"
