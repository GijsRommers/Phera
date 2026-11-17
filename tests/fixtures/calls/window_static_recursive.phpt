--TEST--
Call window: static recursive
--FILE--
<?php
function f($n){static $x=0;$x++;if($n)f($n-1);echo "$n:$x\n";}f(2);f(1);
--EXPECT--
0:3
1:3
2:3
0:5
1:5
