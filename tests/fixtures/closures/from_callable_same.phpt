--TEST--
from callable same
--FILE--
<?php
$f=function(){return 1;};var_dump(Closure::fromCallable($f)===$f);
--EXPECT--
bool(true)
