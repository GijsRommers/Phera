--TEST--
from callable native
--FILE--
<?php
$f=Closure::fromCallable("strlen");var_dump($f instanceof Closure,$f("hello"));
--EXPECT--
bool(true)
int(5)
