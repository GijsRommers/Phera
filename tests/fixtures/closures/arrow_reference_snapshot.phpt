--TEST--
arrow reference snapshot
--FILE--
<?php
$x=3;$y=&$x;$f=fn()=>$y;$x=8;var_dump($f());
--EXPECT--
int(3)
