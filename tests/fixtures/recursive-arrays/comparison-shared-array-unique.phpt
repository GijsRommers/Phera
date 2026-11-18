--TEST--
array-unique compares shared recursive arrays
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = $a;
var_dump(count(array_unique([$a, $b], SORT_REGULAR)));
--EXPECT--
int(1)
