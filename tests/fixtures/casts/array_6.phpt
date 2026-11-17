--TEST--
array_6
--FILE--
<?php
$a=(array)3.25; foreach($a as $x) echo $x,"|";
--EXPECT--
3.25|