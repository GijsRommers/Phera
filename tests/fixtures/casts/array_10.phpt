--TEST--
array_10
--FILE--
<?php
$a=(array)'12.5'; foreach($a as $x) echo $x,"|";
--EXPECT--
12.5|