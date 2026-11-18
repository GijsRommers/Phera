--TEST--
Globals scope
--FILE--
<?php
$x = 5; eval('$x = 6;'); var_dump($x);
--EXPECT--
int(6)
