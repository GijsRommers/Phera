--TEST--
anonymous-classes: implemented interface method
--FILE--
<?php
interface I { function id(): int; } echo (new class implements I { function id(): int { return 2; } })->id();
--EXPECT--
2
