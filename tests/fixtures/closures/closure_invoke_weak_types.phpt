--TEST--
closure invoke weak types
--FILE--
<?php
declare(strict_types=1);$f=fn(int $x)=>$x;var_dump($f->__invoke("3"),call_user_func([$f,"__invoke"],"4"));
--EXPECT--
int(3)
int(4)
