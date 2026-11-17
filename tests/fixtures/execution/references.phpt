--TEST--
references
--FILE--
<?php
$a=1;$b=&$a;$b=2;echo $a;unset($b);$b=3;echo $a,$b;
--EXPECT--
223