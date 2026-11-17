--TEST--
weak_types
--FILE--
<?php
function f(int $a,float $b,string $c,bool $d){echo $a,'|',$b,'|',$c,'|',(int)$d;}f('12','3.5',42,1);
--EXPECT--
12|3.5|42|1