--TEST--
Call window: reference expression error
--FILE--
<?php
function f(&$x){}$x=3;try{f($x+1);}catch(Throwable $e){echo $e->getMessage();}
--EXPECT--
f(): Argument #1 ($x) could not be passed by reference
