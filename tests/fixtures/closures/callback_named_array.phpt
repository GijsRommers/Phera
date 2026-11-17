--TEST--
callback named array
--FILE--
<?php
$f=fn($a=2,$b=3)=>$a*10+$b;var_dump(call_user_func_array($f,["b"=>9]));
--EXPECT--
int(29)
