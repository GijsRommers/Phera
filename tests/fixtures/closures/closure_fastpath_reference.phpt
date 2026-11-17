--TEST--
closure fastpath reference
--FILE--
<?php
$a=2;$f=fn($x)=>$x+$a;$x=3;$y=&$x;var_dump($f($y));
--EXPECT--
int(5)
