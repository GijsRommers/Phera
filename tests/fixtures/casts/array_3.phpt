--TEST--
array_3
--FILE--
<?php
$a=(array)0; foreach($a as $x) echo $x,"|";
--EXPECT--
0|