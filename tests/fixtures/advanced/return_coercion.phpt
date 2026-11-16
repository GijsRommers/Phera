--TEST--
return_coercion
--FILE--
<?php
function f():int{return '42';}echo f();
--EXPECT--
42