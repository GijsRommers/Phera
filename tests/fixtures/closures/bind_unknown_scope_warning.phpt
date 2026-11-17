--TEST--
bind unknown scope warning
--FILE--
<?php
set_error_handler(function($n,$m){echo $m,"
";return true;});$f=fn()=>1;var_dump($f->bindTo(null,"MissingClosureScope"));
--EXPECT--
Class "MissingClosureScope" not found
NULL
