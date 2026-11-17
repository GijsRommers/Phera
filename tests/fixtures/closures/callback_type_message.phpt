--TEST--
callback type message
--FILE--
<?php
$f=fn(int $x)=>$x;try{array_map($f,[[]]);}catch(TypeError $e){echo substr($e->getMessage(),strpos($e->getMessage(),"():")),"
";}
--EXPECT--
(): Argument #1 ($x) must be of type int, array given
