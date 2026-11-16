--TEST--
Classes: self-name
--FILE--
<?php class Box { function name() { return self::class; } } echo (new Box)->name();
--EXPECT--
Box