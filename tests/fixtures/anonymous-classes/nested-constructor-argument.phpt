--TEST--
anonymous-classes: nested constructor argument
--FILE--
<?php
new class(new class {}) { function __construct($inner) { var_dump(get_class($this) !== get_class($inner)); } };
--EXPECT--
bool(true)
