--TEST--
anonymous-classes: implicit public readonly promotion
--FILE--
<?php
echo (new class(2) { function __construct(readonly int $x){} })->x;
--EXPECT--
2
