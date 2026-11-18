--TEST--
Compare a recursive array with itself using ===
--FILE--
<?php
$a = [1];
$a[5] = &$a;
var_dump($a === $a);
--EXPECT--
bool(true)
