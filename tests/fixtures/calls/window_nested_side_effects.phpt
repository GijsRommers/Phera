--TEST--
Call window: nested side effects
--FILE--
<?php
function f($a,$b){echo "f:$a,$b\n";} function g(&$x){echo "g\n";return ++$x;} function h(&$x){echo "h\n";return ++$x;} $x=0; f(g($x),h($x)); echo $x;
--EXPECT--
g
h
f:1,2
2
