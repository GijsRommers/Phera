--TEST--
bind internal scope warning
--FILE--
<?php
set_error_handler(function($n,$m){echo $m,"
";return true;});$f=function(){};var_dump($f->bindTo(null,stdClass::class));
--EXPECT--
Cannot bind closure to scope of internal class stdClass, this will be an error in PHP 9
NULL
