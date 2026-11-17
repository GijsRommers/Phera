--TEST--
bind method scope warning
--FILE--
<?php
set_error_handler(function($n,$m){echo $m,"
";return true;});class A{function f(){}}class B{}$f=(new A)->f(...);var_dump($f->bindTo(new A,B::class));
--EXPECT--
Cannot rebind scope of closure created from method, this will be an error in PHP 9
NULL
