--TEST--
Nested eval shares locals
--FILE--
<?php
$x = 1; eval('eval("\\$x = 8;");'); var_dump($x);
--EXPECT--
int(8)
