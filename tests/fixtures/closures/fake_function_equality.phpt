--TEST--
fake function equality
--FILE--
<?php
function f(){static $i=0;return ++$i;}$a=f(...);$b=Closure::fromCallable("f");$a();var_dump($a==$b,$a==clone $a,$a===$b,$b());
--EXPECT--
bool(true)
bool(true)
bool(false)
int(2)
