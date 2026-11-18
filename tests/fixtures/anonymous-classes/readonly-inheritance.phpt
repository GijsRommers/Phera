--TEST--
anonymous-classes: readonly inheritance
--FILE--
<?php
readonly class P { function __construct(public int $value) {} } echo (new readonly class(4) extends P {})->value;
--EXPECT--
4
