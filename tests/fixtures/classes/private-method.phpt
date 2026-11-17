--TEST--
Classes: private-method
--FILE--
<?php class Box { private function answer() { return 42; } function call() { return $this->answer(); } } echo (new Box)->call();
--EXPECT--
42