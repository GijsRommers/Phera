--TEST--
Call window: nested fifth throws
--FILE--
<?php
function f($a,$b){return $a+$b;} function bad(){throw new Exception('fifth');} try{f(1,f(2,f(3,f(4,bad()))));}catch(Throwable $e){echo $e->getMessage(),"\n";} echo f(7,8);
--EXPECT--
fifth
15
