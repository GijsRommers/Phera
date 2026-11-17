--TEST--
Call window: object ten levels
--FILE--
<?php
function f($x,$n){return $n?f($x,$n-1):$x;}$x=(object)['v'=>7];$y=f($x,10);var_dump($x===$y,$y->v);
--EXPECT--
bool(true)
int(7)
