--TEST--
Deep finite arrays can compare equal
--FILE--
<?php
$a = 0;
$b = 0;
for ($i = 0; $i < 2_000; $i++) {
    $a = [$a];
    $b = [$b];
}
var_dump($a == $b, $a === $b);
--EXPECT--
bool(true)
bool(true)
