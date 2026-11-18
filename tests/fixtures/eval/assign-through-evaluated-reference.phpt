--TEST--
Assign through evaluated reference
--FILE--
<?php
$a=1; $b=2; eval('$a =& $b;'); $a=3; var_dump($b);
--EXPECT--
int(3)
