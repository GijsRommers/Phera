--TEST--
closure callback user func
--FILE--
<?php
$x=4;$f=function($a,$b)use($x){return $a+$b+$x;};var_dump(call_user_func($f,1,2),call_user_func_array($f,[3,4]));
--EXPECT--
int(7)
int(11)
