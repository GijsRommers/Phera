--TEST--
anonymous-classes: promoted private property
--FILE--
<?php
echo (new class(3) { function __construct(private int $x) {} function val() { return $this->x; } })->val();
--EXPECT--
3
