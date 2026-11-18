--TEST--
anonymous-classes: constructor default
--FILE--
<?php
echo (new class { function __construct(int $x = 8) { echo $x; } })->nothing ?? "";
--EXPECT--
8
