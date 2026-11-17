--TEST--
Call window: spread duplicate fixed
--FILE--
<?php
function f($a,$b){}try{f(...['a'=>1],...['a'=>2]);}catch(Throwable $e){echo $e->getMessage();}
--EXPECT--
Named parameter $a overwrites previous argument
