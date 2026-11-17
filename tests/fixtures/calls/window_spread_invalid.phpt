--TEST--
Call window: spread invalid
--FILE--
<?php
function f(...$rest){}try{f(...4);}catch(Throwable $e){echo $e->getMessage();}
--EXPECT--
Only arrays and Traversables can be unpacked, int given
