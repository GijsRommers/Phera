--TEST--
Call window: named reordered
--FILE--
<?php
function f($a,$b,$c=3){echo "$a,$b,$c";} f(c:7,b:5,a:4);
--EXPECT--
4,5,7
