--TEST--
Classes: method-static-instance-call
--FILE--
<?php class Box { static function answer() { return 42; } } echo (new Box)->answer();
--EXPECT--
42