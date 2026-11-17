--TEST--
callback relative class
--FILE--
<?php
set_error_handler(function($n,$m){echo $m,"
";return true;});class A{function f(){return static::class;}function get(){return Closure::fromCallable("self::f");}}$f=(new A)->get();var_dump($f());
--EXPECT--
Use of "self" in callables is deprecated
string(1) "A"
