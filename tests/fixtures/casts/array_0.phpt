--TEST--
array_0
--FILE--
<?php
$a=(array)null; foreach($a as $x) echo $x,"|";
--EXPECT--
