--TEST--
named
--FILE--
<?php
function f($x,$y){echo $x,$y;}f(y:2,x:1);
--EXPECT--
12