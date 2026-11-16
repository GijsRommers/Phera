--TEST--
array_unset
--FILE--
<?php
$a=['x'=>['y'=>1,'z'=>2]];unset($a['x']['y']);echo isset($a['x']['y'])?1:0;echo $a['x']['z'];
--EXPECT--
02