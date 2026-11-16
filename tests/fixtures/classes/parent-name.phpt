--TEST--
Classes: parent-name
--FILE--
<?php class Base {} class Child extends Base { function name() { return parent::class; } } echo (new Child)->name();
--EXPECT--
Base