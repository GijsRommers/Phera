--TEST--
array_1
--FILE--
<?php
$a=(array)false; foreach($a as $x) echo $x,"|";
--EXPECT--
|