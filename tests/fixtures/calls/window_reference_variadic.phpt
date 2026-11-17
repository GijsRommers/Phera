--TEST--
Call window: reference variadic
--FILE--
<?php
function f(&...$rest){$rest[0]='a';$rest['x']='b';}$a=1;$b=2;f($a,x:$b);var_dump($a,$b);
--EXPECT--
string(1) "a"
string(1) "b"
