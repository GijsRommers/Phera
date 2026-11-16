--TEST--
void
--FILE--
<?php
function f():void{echo 42;}f();
--EXPECT--
42