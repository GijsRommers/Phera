--TEST--
arrow reference return
--FILE--
<?php
$f=fn &(&$x)=>$x;$x=3;$y=&$f($x);$y=8;var_dump($x);
--EXPECT--
int(8)
