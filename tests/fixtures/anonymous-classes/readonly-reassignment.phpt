--TEST--
anonymous-classes: readonly reassignment
--FILE--
<?php
$x = new readonly class(1) { function __construct(public int $value) {} }; try { $x->value = 2; } catch (Error $e) { echo get_class($e); }
--EXPECT--
Error
