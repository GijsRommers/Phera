--TEST--
closure reference implicit return
--FILE--
<?php
set_error_handler(function($n,$m){echo $m,"
";return true;});$f=function &(){};var_dump($f->__invoke());
--EXPECT--
Only variable references should be returned by reference
NULL
