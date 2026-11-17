--TEST--
closure fastpath float
--FILE--
<?php
$a=2;$f=fn($x)=>$x+$a;var_dump($f(1.5),$f(2),$f("3"));
--EXPECT--
float(3.5)
int(4)
int(5)
