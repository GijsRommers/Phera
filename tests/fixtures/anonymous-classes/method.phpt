--TEST--
anonymous-classes: method
--FILE--
<?php
echo (new class { function answer() { return 42; } })->answer();
--EXPECT--
42
