--TEST--
Call window: callback reference array
--FILE--
<?php
function f(&$x){$x++;}$x=3;call_user_func_array('f',[&$x]);var_dump($x);
--EXPECT--
int(4)
