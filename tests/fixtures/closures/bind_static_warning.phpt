--TEST--
bind static warning
--FILE--
<?php
set_error_handler(function($n,$m){echo $m,"
";return true;});$f=static function(){};var_dump($f->bindTo(new stdClass));
--EXPECT--
Cannot bind an instance to a static closure, this will be an error in PHP 9
NULL
