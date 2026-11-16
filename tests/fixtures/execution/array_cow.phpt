--TEST--
array_cow
--FILE--
<?php
$a=[1,[2]];$b=$a;$b[1][0]=3;echo $a[1][0],$b[1][0];
--EXPECT--
23