--TEST--
Self-referential array passed to key
--FILE--
<?php
$a = [1];
$a[5] = &$a;
end($a);
var_dump(key($a));
--EXPECT--
int(5)
