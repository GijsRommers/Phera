--TEST--
sort can compare two shared recursive arrays
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$values = [$a, $a];
var_dump(sort($values), $values[0] === $a, $values[1] === $a);
--EXPECT--
bool(true)
bool(true)
bool(true)
