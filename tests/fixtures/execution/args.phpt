--TEST--
args
--FILE--
<?php
function f($x){echo func_num_args();foreach(func_get_args() as $a)echo $a;echo func_get_arg(1);}f(1,2);
--EXPECT--
2122