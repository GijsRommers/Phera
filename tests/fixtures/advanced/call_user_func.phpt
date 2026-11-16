--TEST--
call_user_func
--FILE--
<?php
function f($x){return $x*2;}echo call_user_func('f',3);
--EXPECT--
6