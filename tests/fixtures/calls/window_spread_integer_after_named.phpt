--TEST--
Call window: spread integer after named
--FILE--
<?php
function f(...$rest){}try{f(...['x'=>1,0=>2]);}catch(Throwable $e){echo $e->getMessage();}
--EXPECT--
Cannot use positional argument after named argument during unpacking
