--TEST--
gap-return-reference-type
--FILE--
<?php function &f():int{static $x=1;return $x;}$a=&f();$a=9;echo f();
--EXPECT--
9