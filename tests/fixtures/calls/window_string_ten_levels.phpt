--TEST--
Call window: string ten levels
--FILE--
<?php
function f(string $x,int $n):string{return $n?f($x,$n-1):$x.'!';}echo f('abc',10);
--EXPECT--
abc!
