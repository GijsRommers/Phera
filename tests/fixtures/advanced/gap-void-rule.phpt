--TEST--
gap-void-rule
--FILE--
<?php function f():void{return 1;}f();
--EXPECTF--

Fatal error: A void function must not return a value in %s on line 1
