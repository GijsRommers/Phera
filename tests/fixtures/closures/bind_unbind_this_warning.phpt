--TEST--
bind unbind this warning
--FILE--
<?php
set_error_handler(function($n,$m){echo $m,"
";return true;});class A{function f(){return function(){return $this;};}}$f=(new A)->f();var_dump($f->bindTo(null));
--EXPECT--
Cannot unbind $this of closure using $this, this will be an error in PHP 9
NULL
