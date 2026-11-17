--TEST--
unpack
--FILE--
<?php
function f($x,$y){echo $x,$y;}f(...[1,2]);
--EXPECT--
12