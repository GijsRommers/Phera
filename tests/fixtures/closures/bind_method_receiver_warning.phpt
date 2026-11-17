--TEST--
bind method receiver warning
--FILE--
<?php
set_error_handler(function($n,$m){echo $m,"
";return true;});class A{function f(){}}class B{}$f=(new A)->f(...);var_dump($f->bindTo(new B));
--EXPECT--
Cannot bind method A::f() to object of class B, this will be an error in PHP 9
NULL
