--TEST--
capture mixed
--FILE--
<?php
$a=3;$b=4;$f=function($x)use($a,&$b){$b+=$x;return $a+$b;};var_dump($f(2),$b);
--EXPECT--
int(9)
int(6)
