--TEST--
Call window: finally argument throw
--FILE--
<?php
function f($a,$b){return $a+$b;}function g(){try{throw new Exception('x');}finally{echo 'finally';}}try{f(1,g());}catch(Throwable){echo 'caught';}echo f(2,3);
--EXPECT--
finallycaught5
