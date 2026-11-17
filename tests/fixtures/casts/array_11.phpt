--TEST--
array_11
--FILE--
<?php
$a=(array)[]; foreach($a as $x) echo $x,"|";
--EXPECT--
