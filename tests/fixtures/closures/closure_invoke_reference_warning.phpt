--TEST--
closure invoke reference warning
--FILE--
<?php
set_error_handler(function($n,$m){echo $m,"
";return true;});$f=function(&$x){};call_user_func([$f,"__invoke"],null);
--EXPECT--
Closure::__invoke(): Argument #1 ($x) must be passed by reference, value given
