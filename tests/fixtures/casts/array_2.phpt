--TEST--
array_2
--FILE--
<?php
$a=(array)true; foreach($a as $x) echo $x,"|";
--EXPECT--
1|