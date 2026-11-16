--TEST--
argument_snapshot
--FILE--
<?php
function f($a,$b){echo $a,$b;}$a=1;f($a,$a=2);
--EXPECT--
12