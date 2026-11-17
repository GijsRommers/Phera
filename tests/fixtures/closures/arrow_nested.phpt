--TEST--
arrow nested
--FILE--
<?php
$x=5;$f=fn($a)=>fn($b)=>$x+$a+$b;$g=$f(2);var_dump($g(3));
--EXPECT--
int(10)
