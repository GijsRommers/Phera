--TEST--
bind function scope warning
--FILE--
<?php
set_error_handler(function($n,$m){echo $m,"
";return true;});class A{}$f=strlen(...);var_dump($f->bindTo(null,A::class));
--EXPECT--
Cannot rebind scope of closure created from function, this will be an error in PHP 9
NULL
