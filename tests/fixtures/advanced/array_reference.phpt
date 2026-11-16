--TEST--
array_reference
--FILE--
<?php
$a=1;$b=[&$a];$b[0]=2;echo $a;$a=3;echo $b[0];
--EXPECT--
23