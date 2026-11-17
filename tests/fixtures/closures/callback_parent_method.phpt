--TEST--
callback parent method
--FILE--
<?php
set_error_handler(function($n,$m){echo $m,"
";return true;});class A{function f(){return static::class;}}class B extends A{}var_dump(call_user_func([new B,"parent::f"]));
--EXPECT--
Callables of the form ["B", "parent::f"] are deprecated
string(1) "B"
