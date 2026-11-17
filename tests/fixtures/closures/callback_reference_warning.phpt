--TEST--
callback reference warning
--FILE--
<?php
set_error_handler(function($n,$m){echo substr($m,strpos($m,"():")),"
";return true;});$f=function(&$x){return ++$x;};var_dump(call_user_func($f,2));
--EXPECT--
(): Argument #1 ($x) must be passed by reference, value given
int(3)
