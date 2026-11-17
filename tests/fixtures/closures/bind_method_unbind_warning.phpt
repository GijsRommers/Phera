--TEST--
bind method unbind warning
--FILE--
<?php
set_error_handler(function($n,$m){echo $m,"
";return true;});class A{function f(){}}$f=Closure::fromCallable([new A,"f"]);var_dump($f->bindTo(null));
--EXPECT--
Cannot unbind $this of method, this will be an error in PHP 9
NULL
