--TEST--
anonymous-classes: class magic constant
--FILE--
<?php
$x = new class { function name() { return __CLASS__; } }; var_dump($x->name() === get_class($x));
--EXPECT--
bool(true)
