--TEST--
Call window: named variadic duplicate spread
--FILE--
<?php
function f(...$rest){}try{f(...['x'=>1],x:2);}catch(Throwable $e){echo $e->getMessage();}
--EXPECT--
Named parameter $x overwrites previous argument
