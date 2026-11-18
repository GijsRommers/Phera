--TEST--
anonymous-classes: constructor promotion
--FILE--
<?php
echo (new class(8) { function __construct(public int $x) {} })->x;
--EXPECT--
8
