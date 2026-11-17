--TEST--
fake invoke call
--FILE--
<?php
function f($x){return $x+1;}$a=f(...);$b=strlen(...);
var_dump($a->__invoke(3),$b->__invoke("hello"),call_user_func([$a,'__invoke'],4));
--EXPECT--
int(4)
int(5)
int(5)
