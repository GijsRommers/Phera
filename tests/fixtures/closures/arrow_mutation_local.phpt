--TEST--
arrow mutation local
--FILE--
<?php
$x=1;$f=fn()=>++$x;var_dump($f(),$f(),$x);
--EXPECT--
int(2)
int(2)
int(1)
