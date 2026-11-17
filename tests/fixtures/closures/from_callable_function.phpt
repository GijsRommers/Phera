--TEST--
from callable function
--FILE--
<?php
function plus($a,$b=1){return $a+$b;}$f=Closure::fromCallable("plus");var_dump($f instanceof Closure,$f(3));
--EXPECT--
bool(true)
int(4)
