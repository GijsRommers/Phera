--TEST--
bind scope weak scalar
--FILE--
<?php
set_error_handler(function($n,$m){echo $m,"
";return true;});$f=fn()=>1;var_dump($f->bindTo(null,3));
--EXPECT--
Class "3" not found
NULL
