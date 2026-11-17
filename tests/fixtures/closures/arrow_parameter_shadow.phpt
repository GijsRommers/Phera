--TEST--
arrow parameter shadow
--FILE--
<?php
$x=8;$f=fn($x)=>$x+1;var_dump($f(2),$x);
--EXPECT--
int(3)
int(8)
