--TEST--
Call window: named duplicate
--FILE--
<?php
function f($a,$b=2){}try{f(1,a:2);}catch(Throwable $e){echo $e->getMessage();}
--EXPECT--
Named parameter $a overwrites previous argument
