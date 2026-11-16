--TEST--
Classes: late-static-name
--FILE--
<?php class Base { function name() { return static::class; } } class Child extends Base {} echo (new Child)->name();
--EXPECT--
Child