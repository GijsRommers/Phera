--TEST--
array_9
--FILE--
<?php
$a=(array)'abc'; foreach($a as $x) echo $x,"|";
--EXPECT--
abc|