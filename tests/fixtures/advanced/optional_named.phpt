--TEST--
optional_named
--FILE--
<?php
function f($a=1,$b=2){echo $a,$b;}f(b:3);
--EXPECT--
13