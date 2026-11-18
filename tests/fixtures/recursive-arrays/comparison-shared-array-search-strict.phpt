--TEST--
array-search-strict compares shared recursive arrays
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = $a;
var_dump(array_search($a, ['item' => $b], true));
--EXPECT--
string(4) "item"
