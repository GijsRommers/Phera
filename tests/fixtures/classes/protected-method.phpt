--TEST--
Classes: protected-method
--FILE--
<?php class Base { protected function answer() { return 42; } function call() { return $this->answer(); } } echo (new Base)->call();
--EXPECT--
42