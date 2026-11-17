--TEST--
callback nonstatic
--FILE--
<?php
class A{function f(){}}try{call_user_func(["A","f"]);}catch(TypeError $e){echo $e->getMessage(),"
";}
--EXPECT--
call_user_func(): Argument #1 ($callback) must be a valid callback, non-static method A::f() cannot be called statically
