--TEST--
array_7
--FILE--
<?php
$a=(array)''; foreach($a as $x) echo $x,"|";
--EXPECT--
|