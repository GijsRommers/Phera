--TEST--
anonymous-classes: use trait
--FILE--
<?php
trait T { function answer() { return 3; } } echo (new class { use T; })->answer();
--EXPECT--
3
