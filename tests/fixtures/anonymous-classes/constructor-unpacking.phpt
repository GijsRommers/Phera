--TEST--
anonymous-classes: constructor unpacking
--FILE--
<?php
$pair = [1, 2]; new class(...$pair) { function __construct($a, $b) { echo $a, $b; } };
--EXPECT--
12
