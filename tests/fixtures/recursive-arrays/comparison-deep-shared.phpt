--TEST--
A shared array compares immediately even at extreme depth
--FILE--
<?php
$a = 0;
for ($i = 0; $i < 100_000; $i++) { $a = [$a]; }
var_dump($a === $a);
--EXPECT--
bool(true)
