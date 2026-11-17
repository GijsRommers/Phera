--TEST--
call_user_func_array
--FILE--
<?php
function f($x,$y){return $x+$y;}echo call_user_func_array('f',[3,4]);
--EXPECT--
7