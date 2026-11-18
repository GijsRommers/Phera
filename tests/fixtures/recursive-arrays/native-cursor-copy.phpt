--TEST--
Self-referential array passed to cursor-copy
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$copy = $a;
end($a);
var_dump(key($a), key($copy), $copy[5] === $a);
--EXPECT--
int(5)
int(0)
bool(true)
