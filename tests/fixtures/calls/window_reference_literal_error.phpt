--TEST--
Call window: reference literal error
--FILE--
<?php
function f(&$x){}try{f(2);}catch(Throwable $e){echo $e->getMessage();}
--EXPECT--
f(): Argument #1 ($x) could not be passed by reference
