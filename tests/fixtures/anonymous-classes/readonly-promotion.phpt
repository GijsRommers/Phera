--TEST--
anonymous-classes: readonly promotion
--FILE--
<?php
echo (new readonly class(3) { function __construct(public int $value) {} })->value;
--EXPECT--
3
