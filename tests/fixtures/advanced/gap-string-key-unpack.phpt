--TEST--
gap-string-key-unpack
--FILE--
<?php function f($a,$b){echo $a,':',$b;}f(...['b'=>2,'a'=>1]);
--EXPECT--
1:2