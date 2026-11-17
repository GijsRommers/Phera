--TEST--
Exceptions: static_initializer_error
--FILE--
<?php
function run(){static $number=1/0;} try{run();}catch(Throwable $error){echo get_class($error),"|",$error->getMessage();}
--EXPECTF--
DivisionByZeroError|Division by zero