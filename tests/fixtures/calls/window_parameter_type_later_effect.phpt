--TEST--
Call window: parameter type later effect
--FILE--
<?php
function f(int $a,$b){} function side(){echo 'side';return 2;}try{f([],side());}catch(Throwable $e){echo get_class($e),"\n",$e->getMessage();}
--EXPECTF--
sideTypeError
f(): Argument #1 ($a) must be of type int, array given, called in %s on line 2
