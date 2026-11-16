--TEST--
array_5
--FILE--
<?php
$a=(array)-2; foreach($a as $x) echo $x,"|";
--EXPECT--
-2|