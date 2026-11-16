--TEST--
array_12
--FILE--
<?php
$a=(array)[1]; foreach($a as $x) echo $x,"|";
--EXPECT--
1|