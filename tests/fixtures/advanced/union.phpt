--TEST--
union
--FILE--
<?php
function f(int|string $x):int|string{return $x;}echo f('x'),f(3);
--EXPECT--
x3