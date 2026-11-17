--TEST--
callback reference array
--FILE--
<?php
$f=function(&$a){$a=8;};$a=1;call_user_func_array($f,[&$a]);var_dump($a);
--EXPECT--
int(8)
