--TEST--
arrow named
--FILE--
<?php
$x=3;$f=fn($a,$b=2)=>$x+$a*$b;var_dump($f(b:4,a:5));
--EXPECT--
int(23)
